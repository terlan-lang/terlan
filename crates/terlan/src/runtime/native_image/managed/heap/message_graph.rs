//! Atomic transfer and retention of actor-owned immutable graphs.

use super::*;

impl ActorHeap {
    /// Copies one immutable graph atomically into a receiver-owned mailbox fragment.
    pub(crate) fn copy_message_graph_to(
        &self,
        root: TvmRef<()>,
        expected_type: SemanticTypeId,
        receiver: &mut ActorHeap,
        fragment_id: u32,
        work_budget_bytes: usize,
    ) -> Result<ManagedMailboxFragment, ManagedMemoryError> {
        if self.owner == receiver.owner || fragment_id == 0 || work_budget_bytes == 0 {
            return Err(ManagedMemoryError::InvalidMailboxTransfer);
        }
        let root_offset = self.resolve_offset(root)?;
        if self
            .objects
            .get(&root_offset)
            .ok_or(ManagedMemoryError::UnknownReference)?
            .descriptor
            .semantic_id()
            != expected_type
        {
            return Err(ManagedMemoryError::ManagedTypeMismatch);
        }
        let (copy_order, copied_payload_bytes) =
            self.message_copy_order(root_offset, work_budget_bytes)?;
        let receiver_bytes_before = receiver.space.len();
        let mut staged = ActorHeap {
            owner: receiver.owner,
            token: receiver.token,
            next_reserved_token: receiver.next_reserved_token,
            reserved_tokens_remaining: receiver.reserved_tokens_remaining,
            latest_retired_token: receiver.latest_retired_token,
            retired_tokens: receiver.retired_tokens.clone(),
            limits: receiver.limits,
            next_collection_bytes: receiver.next_collection_bytes,
            space: receiver.space.clone(),
            objects: receiver.objects.clone(),
            external_strings: receiver.external_strings.clone(),
            collections: receiver.collections,
            reuse_underutilized_count: receiver.reuse_underutilized_count,
        };
        let mut relocated = BTreeMap::<usize, TvmRef<()>>::new();
        for source_offset in &copy_order {
            let metadata = self
                .objects
                .get(source_offset)
                .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
            let payload = self
                .space
                .get(*source_offset..*source_offset + metadata.descriptor.size())
                .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?
                .to_vec();
            let references = metadata
                .descriptor
                .reference_offsets()
                .iter()
                .map(|reference_offset| {
                    let encoded = read_reference(&self.space, source_offset + reference_offset)?;
                    let child_offset = self.resolve_encoded(encoded)?;
                    relocated
                        .get(&child_offset)
                        .copied()
                        .map(|reference| (*reference_offset, reference))
                        .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let copied =
                staged.allocate::<()>(metadata.descriptor.clone(), &payload, &references)?;
            if let Some(external) = self.external_strings.get(source_offset) {
                staged.remember_external_string(copied, external.clone())?;
            }
            relocated.insert(*source_offset, copied);
        }
        let copied_root = relocated
            .get(&root_offset)
            .copied()
            .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
        let receiver_heap_bytes = staged
            .space
            .len()
            .checked_sub(receiver_bytes_before)
            .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
        let root = ManagedRoot::new(
            staged.owner,
            RootLocation::Mailbox {
                fragment: fragment_id,
                slot: 0,
            },
            copied_root,
        );
        let fragment = ManagedMailboxFragment::new(
            self.owner,
            staged.owner,
            fragment_id,
            root,
            copy_order.len(),
            copied_payload_bytes,
            receiver_heap_bytes,
        );
        *receiver = staged;
        Ok(fragment)
    }

    /// Retains one immutable same-owner graph as a precise mailbox root.
    pub(crate) fn retain_message_graph(
        &self,
        root: TvmRef<()>,
        expected_type: SemanticTypeId,
        fragment_id: u32,
    ) -> Result<ManagedMailboxFragment, ManagedMemoryError> {
        if fragment_id == 0 {
            return Err(ManagedMemoryError::InvalidMailboxTransfer);
        }
        let root_offset = self.resolve_offset(root)?;
        if self
            .objects
            .get(&root_offset)
            .ok_or(ManagedMemoryError::UnknownReference)?
            .descriptor
            .semantic_id()
            != expected_type
        {
            return Err(ManagedMemoryError::ManagedTypeMismatch);
        }
        Ok(ManagedMailboxFragment::new(
            self.owner,
            self.owner,
            fragment_id,
            ManagedRoot::new(
                self.owner,
                RootLocation::Mailbox {
                    fragment: fragment_id,
                    slot: 0,
                },
                root,
            ),
            0,
            0,
            0,
        ))
    }

    /// Rolls back the most recently copied cross-owner mailbox graph.
    pub(crate) fn rollback_message_graph(
        &mut self,
        fragment: &ManagedMailboxFragment,
    ) -> Result<(), ManagedMemoryError> {
        if fragment.receiver() != self.owner {
            return Err(ManagedMemoryError::InvalidMailboxTransfer);
        }
        let copied_bytes = fragment.receiver_heap_bytes();
        if copied_bytes == 0 {
            return Ok(());
        }
        let start = self
            .space
            .len()
            .checked_sub(copied_bytes)
            .ok_or(ManagedMemoryError::InvalidMailboxTransfer)?;
        let root_offset = self.resolve_offset(fragment.root_reference())?;
        if root_offset < start {
            return Err(ManagedMemoryError::InvalidMailboxTransfer);
        }
        self.space.truncate(start);
        self.objects.truncate_from(start);
        self.external_strings.retain(|offset, _| *offset < start);
        Ok(())
    }

    /// Produces a child-first distinct-object copy order under a hard work budget.
    pub(super) fn message_copy_order(
        &self,
        root_offset: usize,
        work_budget_bytes: usize,
    ) -> Result<(Vec<usize>, usize), ManagedMemoryError> {
        let mut stack = vec![(root_offset, false)];
        let mut visiting = BTreeSet::new();
        let mut completed = BTreeSet::new();
        let mut order = Vec::new();
        let mut work_bytes = 0_usize;
        let mut payload_bytes = 0_usize;
        while let Some((offset, expanded)) = stack.pop() {
            if expanded {
                if !visiting.remove(&offset) || !completed.insert(offset) {
                    continue;
                }
                let metadata = self
                    .objects
                    .get(&offset)
                    .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
                let external_bytes = self.external_strings.get(&offset).map_or(0, Bytes::len);
                payload_bytes = payload_bytes
                    .checked_add(metadata.descriptor.size() + external_bytes)
                    .ok_or(ManagedMemoryError::MessageTransferBudgetExceeded)?;
                work_bytes = work_bytes
                    .checked_add(metadata.descriptor.size() + external_bytes + METADATA_WORK_BYTES)
                    .ok_or(ManagedMemoryError::MessageTransferBudgetExceeded)?;
                if work_bytes > work_budget_bytes {
                    return Err(ManagedMemoryError::MessageTransferBudgetExceeded);
                }
                order.push(offset);
                continue;
            }
            if completed.contains(&offset) {
                continue;
            }
            if !visiting.insert(offset) {
                return Err(ManagedMemoryError::InvalidMailboxTransfer);
            }
            let metadata = self
                .objects
                .get(&offset)
                .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
            stack.push((offset, true));
            for reference_offset in metadata.descriptor.reference_offsets().iter().rev() {
                let encoded = read_reference(&self.space, offset + reference_offset)?;
                let child = self.resolve_encoded(encoded)?;
                if visiting.contains(&child) {
                    return Err(ManagedMemoryError::InvalidMailboxTransfer);
                }
                if !completed.contains(&child) {
                    stack.push((child, false));
                }
            }
        }
        Ok((order, payload_bytes))
    }
}

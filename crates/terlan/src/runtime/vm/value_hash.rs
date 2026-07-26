use super::ReplValue;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Stable VM hashing failure exposed to runtime consumers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmStableHashError {
    #[cfg(test)]
    UnsupportedValue(&'static str),
}

impl ReplValue {
    /// Computes a deterministic, type-separated fingerprint for a portable VM value.
    ///
    /// Transformation:
    /// - Uses explicit tags and fixed little-endian framing instead of Rust enum
    ///   discriminants or process-randomized collection hashing.
    pub(crate) fn stable_hash(&self) -> Result<u64, VmStableHashError> {
        hash_value(self)
    }
}

#[derive(Clone, Copy)]
struct StableHasher {
    state: u64,
}

impl StableHasher {
    fn new(tag: u8) -> Self {
        let mut hasher = Self {
            state: FNV_OFFSET_BASIS,
        };
        hasher.write_byte(tag);
        hasher
    }

    fn write_byte(&mut self, byte: u8) {
        self.state ^= u64::from(byte);
        self.state = self.state.wrapping_mul(FNV_PRIME);
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        self.write_u64(bytes.len() as u64);
        for byte in bytes {
            self.write_byte(*byte);
        }
    }

    fn write_str(&mut self, value: &str) {
        self.write_bytes(value.as_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.write_byte(byte);
        }
    }

    fn finish(self) -> u64 {
        self.state
    }
}

fn hash_value(value: &ReplValue) -> Result<u64, VmStableHashError> {
    enum Task<'a> {
        Value(&'a ReplValue),
        FinishOrdered {
            tag: u8,
            len: usize,
        },
        FinishRecord {
            name: &'a str,
            fields: &'a [(String, ReplValue)],
        },
        FinishMap {
            len: usize,
        },
        FinishSet {
            len: usize,
        },
    }

    let mut tasks = vec![Task::Value(value)];
    let mut results = Vec::<u64>::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Value(value) => {
                let mut hasher = StableHasher::new(value_tag(value));
                match value {
                    ReplValue::Unit => results.push(hasher.finish()),
                    ReplValue::Int(value) => {
                        hasher.write_u64(*value as u64);
                        results.push(hasher.finish());
                    }
                    ReplValue::Float(value)
                    | ReplValue::String(value)
                    | ReplValue::Atom(value)
                    | ReplValue::Type(value) => {
                        hasher.write_str(value);
                        results.push(hasher.finish());
                    }
                    ReplValue::StringBytes(value) => {
                        hasher.write_bytes(value);
                        results.push(hasher.finish());
                    }
                    ReplValue::Bytes(value) => {
                        hasher.write_bytes(value);
                        results.push(hasher.finish());
                    }
                    ReplValue::BitString(value) => {
                        hasher.write_u64(value.bit_len() as u64);
                        hasher.write_bytes(value.packed_bytes());
                        results.push(hasher.finish());
                    }
                    ReplValue::Bool(value) => {
                        hasher.write_byte(u8::from(*value));
                        results.push(hasher.finish());
                    }
                    #[cfg(test)]
                    ReplValue::RandomGenerator(value) => {
                        hasher.write_str(&value.fingerprint());
                        results.push(hasher.finish());
                    }
                    ReplValue::Tuple(items) | ReplValue::List(items) => {
                        tasks.push(Task::FinishOrdered {
                            tag: value_tag(value),
                            len: items.len(),
                        });
                        tasks.extend(items.iter().rev().map(Task::Value));
                    }
                    ReplValue::Record { name, fields } => {
                        tasks.push(Task::FinishRecord { name, fields });
                        tasks.extend(fields.iter().rev().map(|(_, value)| Task::Value(value)));
                    }
                    ReplValue::Map(entries) => {
                        tasks.push(Task::FinishMap { len: entries.len() });
                        for (key, value) in entries.iter().rev() {
                            tasks.push(Task::Value(value));
                            tasks.push(Task::Value(key));
                        }
                    }
                    #[cfg(test)]
                    ReplValue::MapIndexed(map) => {
                        write_unordered_entries(&mut hasher, &map.to_entries())?;
                        results.push(hasher.finish());
                    }
                    ReplValue::Set(items) => {
                        tasks.push(Task::FinishSet { len: items.len() });
                        tasks.extend(items.iter().rev().map(Task::Value));
                    }
                    #[cfg(test)]
                    ReplValue::Iterator { .. } => {
                        return Err(VmStableHashError::UnsupportedValue("Iterator"));
                    }
                }
            }
            Task::FinishOrdered { tag, len } => {
                let hashes = take_results(&mut results, len);
                let mut hasher = StableHasher::new(tag);
                hasher.write_u64(len as u64);
                for hash in hashes {
                    hasher.write_u64(hash);
                }
                results.push(hasher.finish());
            }
            Task::FinishRecord { name, fields } => {
                let hashes = take_results(&mut results, fields.len());
                let mut hasher = StableHasher::new(16);
                hasher.write_str(name);
                hasher.write_u64(fields.len() as u64);
                for ((field, _), hash) in fields.iter().zip(hashes) {
                    hasher.write_str(field);
                    hasher.write_u64(hash);
                }
                results.push(hasher.finish());
            }
            Task::FinishMap { len } => {
                let child_hashes = take_results(&mut results, len.saturating_mul(2));
                let mut entry_hashes = child_hashes
                    .chunks_exact(2)
                    .map(|pair| {
                        let mut entry_hasher = StableHasher::new(1);
                        entry_hasher.write_u64(pair[0]);
                        entry_hasher.write_u64(pair[1]);
                        entry_hasher.finish()
                    })
                    .collect::<Vec<_>>();
                entry_hashes.sort_unstable();
                let mut hasher = StableHasher::new(18);
                hasher.write_u64(len as u64);
                for hash in entry_hashes {
                    hasher.write_u64(hash);
                }
                results.push(hasher.finish());
            }
            Task::FinishSet { len } => {
                let mut hashes = take_results(&mut results, len);
                hashes.sort_unstable();
                let mut hasher = StableHasher::new(19);
                hasher.write_u64(len as u64);
                for hash in hashes {
                    hasher.write_u64(hash);
                }
                results.push(hasher.finish());
            }
        }
    }
    results
        .pop()
        .filter(|_| results.is_empty())
        .ok_or_else(invalid_hash_state)
}

fn take_results(results: &mut Vec<u64>, len: usize) -> Vec<u64> {
    results.split_off(results.len() - len)
}

fn invalid_hash_state() -> VmStableHashError {
    #[cfg(test)]
    {
        VmStableHashError::UnsupportedValue("invalid stable hash traversal state")
    }
    #[cfg(not(test))]
    {
        unreachable!("stable hash traversal must produce exactly one result")
    }
}

fn value_tag(value: &ReplValue) -> u8 {
    match value {
        ReplValue::Unit => 1,
        ReplValue::Int(_) => 2,
        ReplValue::Float(_) => 3,
        ReplValue::String(_) | ReplValue::StringBytes(_) => 4,
        ReplValue::Bytes(_) => 5,
        ReplValue::BitString(_) => 6,
        ReplValue::Atom(_) => 7,
        ReplValue::Bool(_) => 8,
        #[cfg(test)]
        ReplValue::RandomGenerator(_) => 11,
        ReplValue::Type(_) => 14,
        ReplValue::Tuple(_) => 15,
        ReplValue::Record { .. } => 16,
        ReplValue::List(_) => 17,
        ReplValue::Map(_) => 18,
        #[cfg(test)]
        ReplValue::MapIndexed(_) => 18,
        ReplValue::Set(_) => 19,
        #[cfg(test)]
        ReplValue::Iterator { .. } => 20,
    }
}

#[cfg(test)]
fn write_unordered_entries(
    hasher: &mut StableHasher,
    entries: &[(ReplValue, ReplValue)],
) -> Result<(), VmStableHashError> {
    let mut hashes = entries
        .iter()
        .map(|(key, value)| {
            let mut entry_hasher = StableHasher::new(1);
            entry_hasher.write_u64(hash_value(key)?);
            entry_hasher.write_u64(hash_value(value)?);
            Ok(entry_hasher.finish())
        })
        .collect::<Result<Vec<_>, VmStableHashError>>()?;
    hashes.sort_unstable();
    hasher.write_u64(hashes.len() as u64);
    for hash in hashes {
        hasher.write_u64(hash);
    }
    Ok(())
}

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
    let mut hasher = StableHasher::new(value_tag(value));
    match value {
        ReplValue::Unit => {}
        ReplValue::Int(value) => hasher.write_u64(*value as u64),
        ReplValue::Float(value)
        | ReplValue::String(value)
        | ReplValue::Atom(value)
        | ReplValue::Type(value) => hasher.write_str(value),
        ReplValue::Bytes(value) => hasher.write_bytes(value),
        ReplValue::BitString(value) => {
            hasher.write_u64(value.bit_len() as u64);
            hasher.write_bytes(value.packed_bytes());
        }
        ReplValue::Bool(value) => hasher.write_byte(u8::from(*value)),
        #[cfg(test)]
        ReplValue::RandomGenerator(value) => hasher.write_str(&value.fingerprint()),
        ReplValue::Tuple(items) | ReplValue::List(items) => {
            write_ordered_values(&mut hasher, items)?;
        }
        ReplValue::Record { name, fields } => {
            hasher.write_str(name);
            hasher.write_u64(fields.len() as u64);
            for (field, value) in fields {
                hasher.write_str(field);
                hasher.write_u64(hash_value(value)?);
            }
        }
        ReplValue::Map(entries) => write_unordered_entries(&mut hasher, entries)?,
        #[cfg(test)]
        ReplValue::MapIndexed(map) => {
            write_unordered_entries(&mut hasher, &map.to_entries())?;
        }
        ReplValue::Set(items) => write_unordered_values(&mut hasher, items)?,
        #[cfg(test)]
        ReplValue::Iterator { .. } => {
            return Err(VmStableHashError::UnsupportedValue("Iterator"));
        }
    }
    Ok(hasher.finish())
}

fn value_tag(value: &ReplValue) -> u8 {
    match value {
        ReplValue::Unit => 1,
        ReplValue::Int(_) => 2,
        ReplValue::Float(_) => 3,
        ReplValue::String(_) => 4,
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

fn write_ordered_values(
    hasher: &mut StableHasher,
    values: &[ReplValue],
) -> Result<(), VmStableHashError> {
    hasher.write_u64(values.len() as u64);
    for value in values {
        hasher.write_u64(hash_value(value)?);
    }
    Ok(())
}

fn write_unordered_values(
    hasher: &mut StableHasher,
    values: &[ReplValue],
) -> Result<(), VmStableHashError> {
    let mut hashes = values
        .iter()
        .map(hash_value)
        .collect::<Result<Vec<_>, _>>()?;
    hashes.sort_unstable();
    hasher.write_u64(hashes.len() as u64);
    for hash in hashes {
        hasher.write_u64(hash);
    }
    Ok(())
}

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


const DATABASE_BACKED_SQL_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS {table} (record_key TEXT PRIMARY KEY, record TEXT NOT NULL)",
    "SELECT record_key, record FROM {table} ORDER BY record_key",
    "INSERT INTO {table} (record_key, record) VALUES ($1, $2) ON CONFLICT (record_key) DO UPDATE SET record = EXCLUDED.record",
    "DELETE FROM {table} WHERE record_key = $1",
];

fn validate_database_table_name(table_name: &str) -> Result<(), String> {
    if table_name.is_empty() {
        return Err(
            "error[vm_persistent_actor]: persistent actor database table must be non-empty"
                .to_string(),
        );
    }
    if table_name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Ok(())
    } else {
        Err(format!(
            "error[vm_persistent_actor]: persistent actor database table `{table_name}` must use identifier characters"
        ))
    }
}

fn parse_database_backed_row_record(key: &str, value: &str) -> Result<FileBackedRecord, String> {
    parse_embedded_key_value_record(key, value)
}

fn database_snapshot_row_key(actor_id: &VmPersistentActorId) -> String {
    embedded_snapshot_key(actor_id)
}

fn database_event_row_key(actor_id: &VmPersistentActorId, sequence: u64) -> String {
    embedded_event_key(actor_id, sequence)
}

fn parse_embedded_key_value_record(key: &str, value: &str) -> Result<FileBackedRecord, String> {
    let record = parse_file_backed_record(value)?;
    match (
        &record,
        key.strip_prefix("snapshot/"),
        key.strip_prefix("event/"),
    ) {
        (FileBackedRecord::Snapshot(snapshot), Some(encoded_actor), None) => {
            let expected = hex_encode(snapshot.actor_id.0.as_bytes());
            if encoded_actor == expected {
                Ok(record)
            } else {
                Err(format!(
                    "snapshot key actor `{encoded_actor}` does not match record actor `{expected}`"
                ))
            }
        }
        (FileBackedRecord::Event(event), None, Some(rest)) => {
            let Some((encoded_actor, encoded_sequence)) = rest.split_once('/') else {
                return Err("event key must include actor and sequence".to_string());
            };
            let expected_actor = hex_encode(event.actor_id.0.as_bytes());
            let expected_sequence = format!("{:020}", event.sequence);
            if encoded_actor != expected_actor {
                return Err(format!(
                    "event key actor `{encoded_actor}` does not match record actor `{expected_actor}`"
                ));
            }
            if encoded_sequence != expected_sequence {
                return Err(format!(
                    "event key sequence `{encoded_sequence}` does not match record sequence `{expected_sequence}`"
                ));
            }
            Ok(record)
        }
        (FileBackedRecord::Snapshot(_), None, Some(_)) => {
            Err("event key cannot store snapshot record".to_string())
        }
        (FileBackedRecord::Event(_), Some(_), None) => {
            Err("snapshot key cannot store event record".to_string())
        }
        _ => Err("unknown embedded key/value record key".to_string()),
    }
}

fn embedded_snapshot_key(actor_id: &VmPersistentActorId) -> String {
    format!("snapshot/{}", hex_encode(actor_id.0.as_bytes()))
}

fn embedded_event_key(actor_id: &VmPersistentActorId, sequence: u64) -> String {
    format!("event/{}/{sequence:020}", hex_encode(actor_id.0.as_bytes()))
}

fn encode_snapshot_record(snapshot: &VmPersistentActorSnapshot) -> Result<String, String> {
    Ok(format!(
        "snapshot\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        hex_encode(snapshot.actor_id.0.as_bytes()),
        hex_encode(snapshot.schema.id.as_bytes()),
        snapshot.schema.version,
        snapshot.generation,
        snapshot.last_event_sequence,
        encode_repl_value(&snapshot.state)?,
        encode_repl_values(&snapshot.mailbox_checkpoint)?,
        encode_u64_values(&snapshot.timer_checkpoint),
        encode_strings(&snapshot.resource_handles)
    ))
}

fn encode_event_record(event: &VmPersistentActorEvent) -> Result<String, String> {
    Ok(format!(
        "event\t{}\t{}\t{}\t{}\t{}",
        hex_encode(event.actor_id.0.as_bytes()),
        hex_encode(event.schema.id.as_bytes()),
        event.schema.version,
        event.sequence,
        encode_repl_value(&event.payload)?
    ))
}

fn parse_file_backed_record(line: &str) -> Result<FileBackedRecord, String> {
    let fields = line.split('\t').collect::<Vec<_>>();
    match fields.as_slice() {
        ["snapshot", actor, schema_id, schema_version, generation, last_event_sequence, state, mailbox, timers, resources] => {
            Ok(FileBackedRecord::Snapshot(VmPersistentActorSnapshot::new(
                VmPersistentActorId::new(hex_decode_string(actor)?)?,
                VmPersistentActorSchema::new(
                    hex_decode_string(schema_id)?,
                    parse_u64(schema_version, "schema version")?,
                )?,
                parse_u64(generation, "snapshot generation")?,
                parse_repl_value(state)?,
                parse_repl_values(mailbox)?,
                parse_u64_values(timers)?,
                parse_strings(resources)?,
                parse_u64(last_event_sequence, "last event sequence")?,
            )?))
        }
        ["event", actor, schema_id, schema_version, sequence, payload] => {
            Ok(FileBackedRecord::Event(VmPersistentActorEvent::new(
                VmPersistentActorId::new(hex_decode_string(actor)?)?,
                VmPersistentActorSchema::new(
                    hex_decode_string(schema_id)?,
                    parse_u64(schema_version, "schema version")?,
                )?,
                parse_u64(sequence, "event sequence")?,
                parse_repl_value(payload)?,
            )?))
        }
        _ => Err("unknown file-backed record shape".to_string()),
    }
}

fn encode_repl_values(values: &[ReplValue]) -> Result<String, String> {
    values
        .iter()
        .map(encode_repl_value)
        .collect::<Result<Vec<_>, _>>()
        .map(|encoded| encoded.join(","))
}

fn encode_u64_values(values: &[u64]) -> String {
    values
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn encode_strings(values: &[String]) -> String {
    values
        .iter()
        .map(|value| hex_encode(value.as_bytes()))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode_repl_value(value: &ReplValue) -> Result<String, String> {
    match value {
        ReplValue::Unit => Ok("u".to_string()),
        ReplValue::Int(value) => Ok(format!("i:{value}")),
        ReplValue::String(value) => Ok(format!("s:{}", hex_encode(value.as_bytes()))),
        ReplValue::Atom(value) => Ok(format!("a:{}", hex_encode(value.as_bytes()))),
        ReplValue::Bool(value) => Ok(format!("b:{value}")),
        other => Err(format!(
            "error[vm_persistent_actor]: file-backed store cannot encode value `{other:?}`"
        )),
    }
}

fn parse_repl_values(value: &str) -> Result<Vec<ReplValue>, String> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value.split(',').map(parse_repl_value).collect()
}

fn parse_u64_values(value: &str) -> Result<Vec<u64>, String> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|item| parse_u64(item, "u64 list item"))
        .collect()
}

fn parse_strings(value: &str) -> Result<Vec<String>, String> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value.split(',').map(hex_decode_string).collect()
}

fn parse_repl_value(value: &str) -> Result<ReplValue, String> {
    if value == "u" {
        return Ok(ReplValue::Unit);
    }
    if let Some(rest) = value.strip_prefix("i:") {
        return Ok(ReplValue::Int(parse_i64(rest, "integer value")?));
    }
    if let Some(rest) = value.strip_prefix("s:") {
        return Ok(ReplValue::String(hex_decode_string(rest)?));
    }
    if let Some(rest) = value.strip_prefix("a:") {
        return Ok(ReplValue::Atom(hex_decode_string(rest)?));
    }
    if let Some(rest) = value.strip_prefix("b:") {
        return match rest {
            "true" => Ok(ReplValue::Bool(true)),
            "false" => Ok(ReplValue::Bool(false)),
            _ => Err(format!("invalid boolean value `{rest}`")),
        };
    }
    Err(format!("unknown encoded VM value `{value}`"))
}

fn parse_u64(value: &str, label: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|err| format!("invalid {label} `{value}`: {err}"))
}

fn parse_i64(value: &str, label: &str) -> Result<i64, String> {
    value
        .parse::<i64>()
        .map_err(|err| format!("invalid {label} `{value}`: {err}"))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn hex_decode_string(value: &str) -> Result<String, String> {
    String::from_utf8(hex_decode(value)?)
        .map_err(|err| format!("invalid utf-8 in hex string `{value}`: {err}"))
}

fn hex_decode(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err(format!("hex string `{value}` must contain full bytes"));
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for chunk in value.as_bytes().chunks(2) {
        let high = hex_value(chunk[0])?;
        let low = hex_value(chunk[1])?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid hex digit `{}`", byte as char)),
    }
}

fn temporary_file_backed_path(path: &Path) -> PathBuf {
    let mut extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    if !extension.is_empty() {
        extension.push('.');
    }
    extension.push_str("tmp");
    path.with_extension(extension)
}

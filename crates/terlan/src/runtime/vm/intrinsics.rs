use std::collections::HashMap;
use std::fs;

use crate::terlan_typeck::{
    CoreIntrinsicCall, CoreIntrinsicId, CoreModule, CorePrimitiveIntrinsic, CoreRuntimeCapability,
};

use super::{
    compare_bool, compare_string, evaluate_console_println, evaluate_exprs, file_error_value,
    iterator_next, map_from_entries, map_insert, none_value, ok_value, some_value,
    string_binary_predicate, string_predicate, string_unary, type_of_value, unique_values,
    ReplValue,
};

/// Evaluates a supported CoreIR intrinsic call.
///
/// Inputs:
/// - `core`: containing module used for nested evaluation.
/// - `call`: CoreIR intrinsic payload.
/// - `env`: lexical environment.
/// - `output`: callback invoked for console output effects.
///
/// Output:
/// - Intrinsic result value or unsupported-intrinsic error.
///
/// Transformation:
/// - Implements selected target-neutral primitive operations and the first REPL
///   std effect hook directly in the compiler-owned evaluator.
pub(super) fn evaluate_intrinsic(
    core: &CoreModule,
    call: &CoreIntrinsicCall,
    env: &mut HashMap<String, ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let args = evaluate_exprs(core, &call.args, env, output)?;
    match &call.id {
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TypeOf) => {
            let [value] = args.as_slice() else {
                return Err("core.type.type_of expects one argument".to_string());
            };
            Ok(ReplValue::Type(type_of_value(value)))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IsType) => {
            let [value, ReplValue::Type(expected)] = args.as_slice() else {
                return Err("core.type.is_type expects value and Type arguments".to_string());
            };
            Ok(ReplValue::Bool(type_of_value(value) == expected.as_str()))
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ConsolePrintln) => {
            let [value] = args.as_slice() else {
                return Err("runtime.console.println expects one argument".to_string());
            };
            evaluate_console_println(value, output)
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileExists) => {
            let [ReplValue::String(path)] = args.as_slice() else {
                return Err("runtime.file.exists expects String path".to_string());
            };
            Ok(ReplValue::Bool(
                fs::metadata(path)
                    .map(|meta| meta.is_file())
                    .unwrap_or(false),
            ))
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadText) => {
            let [ReplValue::String(path)] = args.as_slice() else {
                return Err("runtime.file.read_text expects String path".to_string());
            };
            Ok(match fs::read_to_string(path) {
                Ok(contents) => ok_value(ReplValue::String(contents)),
                Err(err) => file_error_value(path, &err),
            })
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileWriteText) => {
            let [ReplValue::String(path), ReplValue::String(contents)] = args.as_slice() else {
                return Err("runtime.file.write_text expects String path and content".to_string());
            };
            Ok(match fs::write(path, contents) {
                Ok(()) => ok_value(ReplValue::Unit),
                Err(err) => file_error_value(path, &err),
            })
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileAppendText) => {
            let [ReplValue::String(path), ReplValue::String(contents)] = args.as_slice() else {
                return Err("runtime.file.append_text expects String path and content".to_string());
            };
            let result = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut file| {
                    use std::io::Write;
                    file.write_all(contents.as_bytes())
                });
            Ok(match result {
                Ok(()) => ok_value(ReplValue::Unit),
                Err(err) => file_error_value(path, &err),
            })
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileDelete) => {
            let [ReplValue::String(path)] = args.as_slice() else {
                return Err("runtime.file.delete expects String path".to_string());
            };
            Ok(match fs::remove_file(path) {
                Ok(()) => ok_value(ReplValue::Unit),
                Err(err) => file_error_value(path, &err),
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolEqual) => {
            let [left, right] = args.as_slice() else {
                return Err("core.bool.equal expects two arguments".to_string());
            };
            Ok(ReplValue::Bool(left == right))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolCompare) => {
            compare_bool(args.as_slice())
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IntToString) => {
            let [value] = args.as_slice() else {
                return Err("core.int.to_string expects one argument".to_string());
            };
            match value {
                ReplValue::Int(value) => Ok(ReplValue::String(value.to_string())),
                other => Err(format!(
                    "core.int.to_string expects Int, found {}",
                    other.render()
                )),
            }
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IntFromString) => {
            let [ReplValue::String(value)] = args.as_slice() else {
                return Err("core.int.from_string expects String".to_string());
            };
            Ok(value
                .parse::<i64>()
                .map(|value| some_value(ReplValue::Int(value)))
                .unwrap_or_else(|_| none_value()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::FloatToString) => {
            let [ReplValue::Float(value)] = args.as_slice() else {
                return Err("core.float.to_string expects Float".to_string());
            };
            Ok(ReplValue::String(value.clone()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::FloatFromString) => {
            let [ReplValue::String(value)] = args.as_slice() else {
                return Err("core.float.from_string expects String".to_string());
            };
            Ok(value
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|_| some_value(ReplValue::Float(value.clone())))
                .unwrap_or_else(none_value))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::AtomToString) => {
            let [ReplValue::Atom(value)] = args.as_slice() else {
                return Err("core.atom.to_string expects Atom".to_string());
            };
            Ok(ReplValue::String(value.clone()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringEqual) => {
            let [left, right] = args.as_slice() else {
                return Err("core.string.equal expects two arguments".to_string());
            };
            Ok(ReplValue::Bool(left == right))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringCompare) => {
            compare_string(args.as_slice())
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringToString) => {
            let [value] = args.as_slice() else {
                return Err("core.string.to_string expects one argument".to_string());
            };
            match value {
                ReplValue::String(value) => Ok(ReplValue::String(value.clone())),
                other => Err(format!(
                    "core.string.to_string expects String, found {}",
                    other.render()
                )),
            }
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringFromString) => {
            let [ReplValue::String(value)] = args.as_slice() else {
                return Err("core.string.from_string expects String".to_string());
            };
            Ok(some_value(ReplValue::String(value.clone())))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringIsEmpty) => {
            string_predicate(args.as_slice(), "core.string.is_empty", |value| {
                value.is_empty()
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringAppend) => {
            let [ReplValue::String(left), ReplValue::String(right)] = args.as_slice() else {
                return Err("core.string.append expects two Strings".to_string());
            };
            Ok(ReplValue::String(format!("{left}{right}")))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringConcat) => {
            let [ReplValue::List(values)] = args.as_slice() else {
                return Err("core.string.concat expects List[String]".to_string());
            };
            let mut result = String::new();
            for value in values {
                let ReplValue::String(value) = value else {
                    return Err(format!(
                        "core.string.concat expects String item, found {}",
                        value.render()
                    ));
                };
                result.push_str(value);
            }
            Ok(ReplValue::String(result))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringContains) => {
            string_binary_predicate(args.as_slice(), "core.string.contains", |value, pattern| {
                value.contains(pattern)
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringStartsWith) => {
            string_binary_predicate(
                args.as_slice(),
                "core.string.starts_with",
                |value, pattern| value.starts_with(pattern),
            )
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringEndsWith) => {
            string_binary_predicate(
                args.as_slice(),
                "core.string.ends_with",
                |value, pattern| value.ends_with(pattern),
            )
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolToString) => {
            let [value] = args.as_slice() else {
                return Err("core.bool.to_string expects one argument".to_string());
            };
            match value {
                ReplValue::Bool(value) => Ok(ReplValue::String(value.to_string())),
                other => Err(format!(
                    "core.bool.to_string expects Bool, found {}",
                    other.render()
                )),
            }
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolFromString) => {
            let [ReplValue::String(value)] = args.as_slice() else {
                return Err("core.bool.from_string expects String".to_string());
            };
            Ok(match value.as_str() {
                "true" => some_value(ReplValue::Bool(true)),
                "false" => some_value(ReplValue::Bool(false)),
                _ => none_value(),
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringLength) => {
            let [value] = args.as_slice() else {
                return Err("core.string.length expects one argument".to_string());
            };
            match value {
                ReplValue::String(value) => Ok(ReplValue::Int(value.chars().count() as i64)),
                other => Err(format!(
                    "core.string.length expects String, found {}",
                    other.render()
                )),
            }
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringByteSize) => {
            let [ReplValue::String(value)] = args.as_slice() else {
                return Err("core.string.byte_size expects String".to_string());
            };
            Ok(ReplValue::Int(value.len() as i64))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringUppercase) => {
            string_unary(args.as_slice(), "core.string.uppercase", str::to_uppercase)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringLowercase) => {
            string_unary(args.as_slice(), "core.string.lowercase", str::to_lowercase)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringTrim) => {
            string_unary(args.as_slice(), "core.string.trim", |value| {
                value.trim().to_string()
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringTrimStart) => {
            string_unary(args.as_slice(), "core.string.trim_start", |value| {
                value.trim_start().to_string()
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringTrimEnd) => {
            string_unary(args.as_slice(), "core.string.trim_end", |value| {
                value.trim_end().to_string()
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringReplace) => {
            let [ReplValue::String(value), ReplValue::String(from), ReplValue::String(to)] =
                args.as_slice()
            else {
                return Err("core.string.replace expects value, from, and to Strings".to_string());
            };
            Ok(ReplValue::String(value.replace(from, to)))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringSplit) => {
            let [ReplValue::String(value), ReplValue::String(separator)] = args.as_slice() else {
                return Err("core.string.split expects value and separator Strings".to_string());
            };
            Ok(ReplValue::List(
                value
                    .split(separator)
                    .map(|part| ReplValue::String(part.to_string()))
                    .collect(),
            ))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringSplitOnce) => {
            let [ReplValue::String(value), ReplValue::String(separator)] = args.as_slice() else {
                return Err(
                    "core.string.split_once expects value and separator Strings".to_string()
                );
            };
            Ok(value
                .split_once(separator)
                .map(|(left, right)| {
                    some_value(ReplValue::Tuple(vec![
                        ReplValue::String(left.to_string()),
                        ReplValue::String(right.to_string()),
                    ]))
                })
                .unwrap_or_else(none_value))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListNew) => Ok(ReplValue::List(vec![])),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListIsEmpty) => {
            let [ReplValue::List(items)] = args.as_slice() else {
                return Err("core.list.is_empty expects List".to_string());
            };
            Ok(ReplValue::Bool(items.is_empty()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListLength) => {
            let [ReplValue::List(items)] = args.as_slice() else {
                return Err("core.list.length expects List".to_string());
            };
            Ok(ReplValue::Int(items.len() as i64))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListFirst) => {
            let [ReplValue::List(items)] = args.as_slice() else {
                return Err("core.list.first expects List".to_string());
            };
            Ok(items
                .first()
                .cloned()
                .map(some_value)
                .unwrap_or_else(none_value))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListIterator) => {
            let [ReplValue::List(items)] = args.as_slice() else {
                return Err("core.list.iterator expects List".to_string());
            };
            Ok(ReplValue::Iterator {
                items: items.clone(),
                index: 0,
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListPush) => {
            let [ReplValue::List(items), value] = args.as_slice() else {
                return Err("core.list.push expects List and value".to_string());
            };
            let mut updated = items.clone();
            updated.push(value.clone());
            Ok(ReplValue::List(updated))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListClear) => {
            Ok(ReplValue::List(vec![]))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IteratorNext) => {
            iterator_next(args.as_slice())
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapNew) => Ok(ReplValue::Map(vec![])),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapFromEntries) => {
            let [ReplValue::List(entries)] = args.as_slice() else {
                return Err("core.map.from_entries expects List of entries".to_string());
            };
            map_from_entries(entries.clone())
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapIsEmpty) => {
            let [ReplValue::Map(entries)] = args.as_slice() else {
                return Err("core.map.is_empty expects Map".to_string());
            };
            Ok(ReplValue::Bool(entries.is_empty()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapSize) => {
            let [ReplValue::Map(entries)] = args.as_slice() else {
                return Err("core.map.size expects Map".to_string());
            };
            Ok(ReplValue::Int(entries.len() as i64))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapGet) => {
            let [ReplValue::Map(entries), key] = args.as_slice() else {
                return Err("core.map.get expects Map and key".to_string());
            };
            Ok(entries
                .iter()
                .find(|(entry_key, _)| entry_key == key)
                .map(|(_, value)| some_value(value.clone()))
                .unwrap_or_else(none_value))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapContainsKey) => {
            let [ReplValue::Map(entries), key] = args.as_slice() else {
                return Err("core.map.contains_key expects Map and key".to_string());
            };
            Ok(ReplValue::Bool(
                entries.iter().any(|(entry_key, _)| entry_key == key),
            ))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapIterator) => {
            let [ReplValue::Map(entries)] = args.as_slice() else {
                return Err("core.map.iterator expects Map".to_string());
            };
            Ok(ReplValue::Iterator {
                items: entries
                    .iter()
                    .map(|(key, value)| ReplValue::Tuple(vec![key.clone(), value.clone()]))
                    .collect(),
                index: 0,
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapPut) => {
            let [ReplValue::Map(entries), key, value] = args.as_slice() else {
                return Err("core.map.put expects Map, key, and value".to_string());
            };
            let mut updated = entries.clone();
            map_insert(&mut updated, key.clone(), value.clone());
            Ok(ReplValue::Map(updated))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapRemove) => {
            let [ReplValue::Map(entries), key] = args.as_slice() else {
                return Err("core.map.remove expects Map and key".to_string());
            };
            let mut updated = entries.clone();
            updated.retain(|(entry_key, _)| entry_key != key);
            Ok(ReplValue::Map(updated))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapClear) => Ok(ReplValue::Map(vec![])),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetNew) => Ok(ReplValue::Set(vec![])),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetFromList) => {
            let [ReplValue::List(items)] = args.as_slice() else {
                return Err("core.set.from_list expects List".to_string());
            };
            Ok(ReplValue::Set(unique_values(items.clone())))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetIsEmpty) => {
            let [ReplValue::Set(items)] = args.as_slice() else {
                return Err("core.set.is_empty expects Set".to_string());
            };
            Ok(ReplValue::Bool(items.is_empty()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetSize) => {
            let [ReplValue::Set(items)] = args.as_slice() else {
                return Err("core.set.size expects Set".to_string());
            };
            Ok(ReplValue::Int(items.len() as i64))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetContains) => {
            let [ReplValue::Set(items), value] = args.as_slice() else {
                return Err("core.set.contains expects Set and value".to_string());
            };
            Ok(ReplValue::Bool(items.contains(value)))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetIterator) => {
            let [ReplValue::Set(items)] = args.as_slice() else {
                return Err("core.set.iterator expects Set".to_string());
            };
            Ok(ReplValue::Iterator {
                items: items.clone(),
                index: 0,
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetAdd) => {
            let [ReplValue::Set(items), value] = args.as_slice() else {
                return Err("core.set.add expects Set and value".to_string());
            };
            let mut updated = items.clone();
            if !updated.contains(value) {
                updated.push(value.clone());
            }
            Ok(ReplValue::Set(updated))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetRemove) => {
            let [ReplValue::Set(items), value] = args.as_slice() else {
                return Err("core.set.remove expects Set and value".to_string());
            };
            let mut updated = items.clone();
            updated.retain(|item| item != value);
            Ok(ReplValue::Set(updated))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetClear) => Ok(ReplValue::Set(vec![])),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TaskDone) => {
            let [value] = args.as_slice() else {
                return Err("core.task.done expects one value".to_string());
            };
            Ok(ok_value(value.clone()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TaskResult) => {
            let [task] = args.as_slice() else {
                return Err("core.task.result expects one task".to_string());
            };
            Ok(task.clone())
        }
        other => Err(format!(
            "CoreIR evaluator does not yet support intrinsic {:?}",
            other
        )),
    }
}

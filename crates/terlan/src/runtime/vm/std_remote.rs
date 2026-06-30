use super::{
    compare_bool, compare_string, file_error_record, map_from_entries, none_value, ordering_atom,
    some_value, string_binary_predicate, string_predicate, string_unary, unique_values, ReplValue,
};

/// Evaluates stdlib remote calls that are not loaded as executable modules.
pub(super) fn evaluate_std_remote(
    module: &str,
    function: &str,
    args: Vec<ReplValue>,
) -> Result<ReplValue, String> {
    if module == "__receiver__" {
        return evaluate_receiver_remote(function, args);
    }
    match (module, function) {
        ("List" | "std.collections.List", "new") => Ok(ReplValue::List(vec![])),
        ("Map" | "std.collections.Map" | "Object" | "std.core.Object", "new") => {
            Ok(ReplValue::Map(vec![]))
        }
        ("Set" | "std.collections.Set", "new") => Ok(ReplValue::Set(vec![])),
        ("Map" | "std.collections.Map" | "Object" | "std.core.Object", "from_entries") => {
            let [ReplValue::List(entries)] = args.as_slice() else {
                return Err(format!("{module}.from_entries expects List"));
            };
            map_from_entries(entries.clone())
        }
        ("Set" | "std.collections.Set", "from_list") => {
            let [ReplValue::List(items)] = args.as_slice() else {
                return Err(format!("{module}.from_list expects List"));
            };
            Ok(ReplValue::Set(unique_values(items.clone())))
        }
        ("std.test.Test", "assert") | ("std.test.Test", "assert_true") => {
            expect_unary_bool(module, function, args)
        }
        ("std.test.Test", "assert_false") => {
            let value = expect_unary_bool(module, function, args)?;
            match value {
                ReplValue::Bool(value) => Ok(ReplValue::Bool(!value)),
                _ => unreachable!("expect_unary_bool returns Bool"),
            }
        }
        ("std.test.Test", "assert_equal") => expect_binary_equal(args, true),
        ("std.test.Test", "assert_not_equal") => expect_binary_equal(args, false),
        ("std.test.Test", "fail") if args.is_empty() => Ok(ReplValue::Bool(false)),
        ("std.core.Bool", "equal") => expect_binary_equal(args, true),
        ("std.core.Bool", "is_true") => expect_unary_bool(module, function, args),
        ("std.core.Bool", "is_false") => {
            let value = expect_unary_bool(module, function, args)?;
            let ReplValue::Bool(value) = value else {
                unreachable!("expect_unary_bool returns Bool");
            };
            Ok(ReplValue::Bool(!value))
        }
        ("std.core.Bool", "compare") => compare_bool(args.as_slice()),
        ("std.core.Bool", "to_string") => bool_to_string(args.as_slice()),
        ("std.core.Bool", "from_string") => bool_from_string(args.as_slice()),
        ("std.core.Int", "equal") => expect_binary_equal(args, true),
        ("std.core.Int", "min") => int_minmax(args.as_slice(), true),
        ("std.core.Int", "max") => int_minmax(args.as_slice(), false),
        ("std.core.Int", "abs") => int_abs(args.as_slice()),
        ("std.core.Int", "compare") => int_compare(args.as_slice()),
        ("std.core.Int", "to_string") => int_to_string(args.as_slice()),
        ("std.core.Int", "from_string") => int_from_string(args.as_slice()),
        ("std.core.Float", "equal") => expect_binary_equal(args, true),
        ("std.core.Float", "min") => float_minmax(args.as_slice(), true),
        ("std.core.Float", "max") => float_minmax(args.as_slice(), false),
        ("std.core.Float", "abs") => float_abs(args.as_slice()),
        ("std.core.Float", "compare") => float_compare(args.as_slice()),
        ("std.core.Float", "to_string") => float_to_string(args.as_slice()),
        ("std.core.Float", "from_string") => float_from_string(args.as_slice()),
        ("std.core.String", "equal") => expect_binary_equal(args, true),
        ("std.core.String", "compare") => compare_string(args.as_slice()),
        ("std.core.String", "to_string") => string_identity(args.as_slice()),
        ("std.core.String", "from_string") => string_from_string(args.as_slice()),
        ("std.core.String", "is_empty") => {
            string_predicate(args.as_slice(), "std.core.String.is_empty", |value| {
                value.is_empty()
            })
        }
        ("std.core.String", "append") => string_append(args.as_slice()),
        ("std.core.String", "concat") => string_concat(args.as_slice()),
        ("std.core.String", "contains") => string_binary_predicate(
            args.as_slice(),
            "std.core.String.contains",
            |value, pattern| value.contains(pattern),
        ),
        ("std.core.String", "starts_with") => string_binary_predicate(
            args.as_slice(),
            "std.core.String.starts_with",
            |value, pattern| value.starts_with(pattern),
        ),
        ("std.core.String", "ends_with") => string_binary_predicate(
            args.as_slice(),
            "std.core.String.ends_with",
            |value, pattern| value.ends_with(pattern),
        ),
        ("std.core.String", "length") => string_length(args.as_slice(), false),
        ("std.core.String", "byte_size") => string_length(args.as_slice(), true),
        ("std.core.String", "lowercase") => string_unary(
            args.as_slice(),
            "std.core.String.lowercase",
            str::to_lowercase,
        ),
        ("std.core.String", "uppercase") => string_unary(
            args.as_slice(),
            "std.core.String.uppercase",
            str::to_uppercase,
        ),
        ("std.core.String", "trim") => {
            string_unary(args.as_slice(), "std.core.String.trim", |value| {
                value.trim().to_string()
            })
        }
        ("std.core.String", "trim_start") => {
            string_unary(args.as_slice(), "std.core.String.trim_start", |value| {
                value.trim_start().to_string()
            })
        }
        ("std.core.String", "trim_end") => {
            string_unary(args.as_slice(), "std.core.String.trim_end", |value| {
                value.trim_end().to_string()
            })
        }
        ("std.core.String", "replace") => string_replace(args.as_slice()),
        ("std.core.String", "split") => string_split(args.as_slice()),
        ("std.core.String", "split_once") => string_split_once(args.as_slice()),
        ("std.core.Option", "is_some") => option_is_some(args.as_slice()),
        ("std.core.Option", "is_none") => option_is_none(args.as_slice()),
        ("std.core.Option", "with_default") => option_with_default(args.as_slice()),
        ("std.core.Result", "is_ok") => result_is_ok(args.as_slice()),
        ("std.core.Result", "is_err") => result_is_err(args.as_slice()),
        ("std.core.Result", "with_default") => result_with_default(args.as_slice()),
        ("std.core.Unit", "equal") => Ok(ReplValue::Bool(true)),
        ("std.core.Unit", "compare") => Ok(ReplValue::Atom("eq".to_string())),
        ("std.core.Unit", "to_string") => Ok(ReplValue::String("unit".to_string())),
        ("std.core.Unit", "from_string") => unit_from_string(args.as_slice()),
        ("std.core.Ordering", "compare") => ordering_compare(args.as_slice()),
        ("std.core.Ordering", "to_string") => atom_payload_to_string(args.as_slice()),
        ("std.core.Ordering", "from_string") => ordering_from_string(args.as_slice()),
        ("std.core.Atom", "equal") => expect_binary_equal(args, true),
        ("std.core.Atom", "to_string") => atom_payload_to_string(args.as_slice()),
        ("std.io.File", "new") => file_new(args.as_slice()),
        ("std.io.File", "code") => file_error_field(args.as_slice(), 1, "code"),
        ("std.io.File", "message") => file_error_field(args.as_slice(), 2, "message"),
        ("std.io.File", "path") => file_error_field(args.as_slice(), 3, "path"),
        _ => Err(format!(
            "CoreIR evaluator does not yet support RemoteCall {module}:{function}/{}",
            args.len()
        )),
    }
}

/// Evaluates a VM-facing dynamic receiver call.
fn evaluate_receiver_remote(function: &str, args: Vec<ReplValue>) -> Result<ReplValue, String> {
    let Some((receiver, rest)) = args.split_first() else {
        return Err(format!("receiver call `{function}` requires a receiver"));
    };
    match (function, receiver, rest) {
        ("is_empty", ReplValue::String(value), []) => Ok(ReplValue::Bool(value.is_empty())),
        ("is_empty", ReplValue::List(items), []) => Ok(ReplValue::Bool(items.is_empty())),
        ("is_empty", ReplValue::Map(entries), []) => Ok(ReplValue::Bool(entries.is_empty())),
        ("is_empty", ReplValue::Set(items), []) => Ok(ReplValue::Bool(items.is_empty())),
        ("length", ReplValue::String(value), []) => {
            Ok(ReplValue::Int(value.chars().count() as i64))
        }
        ("length", ReplValue::List(items), []) => Ok(ReplValue::Int(items.len() as i64)),
        ("byte_size", ReplValue::String(value), []) => Ok(ReplValue::Int(value.len() as i64)),
        ("size", ReplValue::Map(entries), []) => Ok(ReplValue::Int(entries.len() as i64)),
        ("size", ReplValue::Set(items), []) => Ok(ReplValue::Int(items.len() as i64)),
        ("first", ReplValue::List(items), []) => Ok(items
            .first()
            .cloned()
            .map(some_value)
            .unwrap_or_else(none_value)),
        ("iterator", ReplValue::List(items), []) | ("iterator", ReplValue::Set(items), []) => {
            Ok(ReplValue::Iterator {
                items: items.clone(),
                index: 0,
            })
        }
        ("iterator", ReplValue::Map(entries), []) => Ok(ReplValue::Iterator {
            items: entries
                .iter()
                .map(|(key, value)| ReplValue::Tuple(vec![key.clone(), value.clone()]))
                .collect(),
            index: 0,
        }),
        ("get", ReplValue::Map(entries), [key]) => Ok(entries
            .iter()
            .find(|(entry_key, _)| entry_key == key)
            .map(|(_, value)| some_value(value.clone()))
            .unwrap_or_else(none_value)),
        ("contains_key", ReplValue::Map(entries), [key]) => Ok(ReplValue::Bool(
            entries.iter().any(|(entry_key, _)| entry_key == key),
        )),
        ("contains", ReplValue::Set(items), [value]) => Ok(ReplValue::Bool(items.contains(value))),
        ("contains", ReplValue::String(value), [ReplValue::String(pattern)]) => {
            Ok(ReplValue::Bool(value.contains(pattern)))
        }
        ("starts_with", ReplValue::String(value), [ReplValue::String(pattern)]) => {
            Ok(ReplValue::Bool(value.starts_with(pattern)))
        }
        ("ends_with", ReplValue::String(value), [ReplValue::String(pattern)]) => {
            Ok(ReplValue::Bool(value.ends_with(pattern)))
        }
        ("append", ReplValue::String(value), [ReplValue::String(suffix)]) => {
            Ok(ReplValue::String(format!("{value}{suffix}")))
        }
        ("lowercase", ReplValue::String(value), []) => Ok(ReplValue::String(value.to_lowercase())),
        ("uppercase", ReplValue::String(value), []) => Ok(ReplValue::String(value.to_uppercase())),
        ("trim", ReplValue::String(value), []) => Ok(ReplValue::String(value.trim().to_string())),
        ("trim_start", ReplValue::String(value), []) => {
            Ok(ReplValue::String(value.trim_start().to_string()))
        }
        ("trim_end", ReplValue::String(value), []) => {
            Ok(ReplValue::String(value.trim_end().to_string()))
        }
        ("replace", ReplValue::String(value), [ReplValue::String(from), ReplValue::String(to)]) => {
            Ok(ReplValue::String(value.replace(from, to)))
        }
        ("split", ReplValue::String(value), [ReplValue::String(separator)]) => Ok(ReplValue::List(
            value
                .split(separator)
                .map(|part| ReplValue::String(part.to_string()))
                .collect(),
        )),
        ("split_once", ReplValue::String(value), [ReplValue::String(separator)]) => Ok(value
            .split_once(separator)
            .map(|(left, right)| {
                some_value(ReplValue::Tuple(vec![
                    ReplValue::String(left.to_string()),
                    ReplValue::String(right.to_string()),
                ]))
            })
            .unwrap_or_else(none_value)),
        ("to_string", value, []) => Ok(ReplValue::String(match value {
            ReplValue::String(value) => value.clone(),
            ReplValue::Bool(value) => value.to_string(),
            ReplValue::Int(value) => value.to_string(),
            ReplValue::Float(value) => value.clone(),
            ReplValue::Atom(value) => value.clone(),
            ReplValue::Unit => "unit".to_string(),
            other => other.render(),
        })),
        _ => Err(format!(
            "CoreIR evaluator does not yet support receiver call `{function}` for {}",
            receiver.render()
        )),
    }
}

/// Expects and returns one Bool argument.
fn expect_unary_bool(
    module: &str,
    function: &str,
    args: Vec<ReplValue>,
) -> Result<ReplValue, String> {
    let [ReplValue::Bool(value)] = args.as_slice() else {
        return Err(format!("{module}.{function} expects Bool"));
    };
    Ok(ReplValue::Bool(*value))
}

/// Evaluates binary equality or inequality.
fn expect_binary_equal(args: Vec<ReplValue>, expected_equal: bool) -> Result<ReplValue, String> {
    let [left, right] = args.as_slice() else {
        return Err("equality helper expects two arguments".to_string());
    };
    Ok(ReplValue::Bool((left == right) == expected_equal))
}

/// Converts Bool to String.
fn bool_to_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Bool(value)] = args else {
        return Err("Bool.to_string expects Bool".to_string());
    };
    Ok(ReplValue::String(value.to_string()))
}

/// Parses Bool from String.
fn bool_from_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("Bool.from_string expects String".to_string());
    };
    Ok(match value.as_str() {
        "true" => some_value(ReplValue::Bool(true)),
        "false" => some_value(ReplValue::Bool(false)),
        _ => none_value(),
    })
}

/// Returns integer min or max.
fn int_minmax(args: &[ReplValue], min: bool) -> Result<ReplValue, String> {
    let [ReplValue::Int(left), ReplValue::Int(right)] = args else {
        return Err("Int min/max expects two Int values".to_string());
    };
    Ok(ReplValue::Int(if min {
        (*left).min(*right)
    } else {
        (*left).max(*right)
    }))
}

/// Returns integer absolute value.
fn int_abs(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Int(value)] = args else {
        return Err("Int.abs expects Int".to_string());
    };
    Ok(ReplValue::Int(value.abs()))
}

/// Compares Int values.
fn int_compare(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Int(left), ReplValue::Int(right)] = args else {
        return Err("Int.compare expects two Int values".to_string());
    };
    Ok(ordering_atom(left.cmp(right)))
}

/// Converts Int to String.
fn int_to_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Int(value)] = args else {
        return Err("Int.to_string expects Int".to_string());
    };
    Ok(ReplValue::String(value.to_string()))
}

/// Parses Int from String.
fn int_from_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("Int.from_string expects String".to_string());
    };
    Ok(value
        .parse::<i64>()
        .map(|value| some_value(ReplValue::Int(value)))
        .unwrap_or_else(|_| none_value()))
}

/// Parses a VM float payload.
fn parse_float(value: &str) -> Result<f64, String> {
    value
        .parse::<f64>()
        .map_err(|err| format!("invalid Float `{value}`: {err}"))
}

/// Returns float min or max.
fn float_minmax(args: &[ReplValue], min: bool) -> Result<ReplValue, String> {
    let [ReplValue::Float(left), ReplValue::Float(right)] = args else {
        return Err("Float min/max expects two Float values".to_string());
    };
    let left_value = parse_float(left)?;
    let right_value = parse_float(right)?;
    Ok(ReplValue::Float(if (left_value <= right_value) == min {
        left.clone()
    } else {
        right.clone()
    }))
}

/// Returns float absolute value.
fn float_abs(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Float(value)] = args else {
        return Err("Float.abs expects Float".to_string());
    };
    let parsed = parse_float(value)?.abs();
    Ok(ReplValue::Float(parsed.to_string()))
}

/// Compares Float values.
fn float_compare(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Float(left), ReplValue::Float(right)] = args else {
        return Err("Float.compare expects two Float values".to_string());
    };
    let ordering = parse_float(left)?
        .partial_cmp(&parse_float(right)?)
        .ok_or_else(|| "Float.compare does not support non-finite values".to_string())?;
    Ok(ordering_atom(ordering))
}

/// Converts Float to String.
fn float_to_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Float(value)] = args else {
        return Err("Float.to_string expects Float".to_string());
    };
    Ok(ReplValue::String(value.clone()))
}

/// Parses Float from String.
fn float_from_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("Float.from_string expects String".to_string());
    };
    Ok(value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(|_| some_value(ReplValue::Float(value.clone())))
        .unwrap_or_else(none_value))
}

/// Returns a String unchanged.
fn string_identity(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("String.to_string expects String".to_string());
    };
    Ok(ReplValue::String(value.clone()))
}

/// Wraps a String in Some.
fn string_from_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    string_identity(args).map(some_value)
}

/// Appends two Strings.
fn string_append(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(left), ReplValue::String(right)] = args else {
        return Err("String.append expects two Strings".to_string());
    };
    Ok(ReplValue::String(format!("{left}{right}")))
}

/// Concatenates a list of strings.
fn string_concat(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::List(values)] = args else {
        return Err("String.concat expects List[String]".to_string());
    };
    let mut result = String::new();
    for value in values {
        let ReplValue::String(value) = value else {
            return Err(format!(
                "String.concat item must be String, found {}",
                value.render()
            ));
        };
        result.push_str(value);
    }
    Ok(ReplValue::String(result))
}

/// Returns String char or byte length.
fn string_length(args: &[ReplValue], bytes: bool) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("String length expects String".to_string());
    };
    Ok(ReplValue::Int(if bytes {
        value.len()
    } else {
        value.chars().count()
    } as i64))
}

/// Replaces all String matches.
fn string_replace(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value), ReplValue::String(from), ReplValue::String(to)] = args else {
        return Err("String.replace expects value, from, and to".to_string());
    };
    Ok(ReplValue::String(value.replace(from, to)))
}

/// Splits a String.
fn string_split(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value), ReplValue::String(separator)] = args else {
        return Err("String.split expects value and separator".to_string());
    };
    Ok(ReplValue::List(
        value
            .split(separator)
            .map(|part| ReplValue::String(part.to_string()))
            .collect(),
    ))
}

/// Splits a String once.
fn string_split_once(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value), ReplValue::String(separator)] = args else {
        return Err("String.split_once expects value and separator".to_string());
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

/// Tests Option presence.
fn option_is_some(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [value] = args else {
        return Err("Option.is_some expects one argument".to_string());
    };
    Ok(ReplValue::Bool(matches!(
        value,
        ReplValue::Tuple(items)
            if matches!(items.as_slice(), [ReplValue::Atom(tag), _] if tag == "some")
    )))
}

/// Tests Option absence.
fn option_is_none(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [value] = args else {
        return Err("Option.is_none expects one argument".to_string());
    };
    Ok(ReplValue::Bool(
        matches!(value, ReplValue::Atom(tag) if tag == "none"),
    ))
}

/// Unwraps Option with a default.
fn option_with_default(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [value, default] = args else {
        return Err("Option.with_default expects value and default".to_string());
    };
    match value {
        ReplValue::Tuple(items) => match items.as_slice() {
            [ReplValue::Atom(tag), value] if tag == "some" => Ok(value.clone()),
            _ => Ok(default.clone()),
        },
        ReplValue::Atom(tag) if tag == "none" => Ok(default.clone()),
        _ => Ok(default.clone()),
    }
}

/// Tests Result success.
fn result_is_ok(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [value] = args else {
        return Err("Result.is_ok expects one argument".to_string());
    };
    Ok(ReplValue::Bool(matches!(
        value,
        ReplValue::Tuple(items)
            if matches!(items.as_slice(), [ReplValue::Atom(tag), _] if tag == "ok")
    )))
}

/// Tests Result error.
fn result_is_err(args: &[ReplValue]) -> Result<ReplValue, String> {
    result_is_ok(args).map(|value| match value {
        ReplValue::Bool(value) => ReplValue::Bool(!value),
        other => other,
    })
}

/// Unwraps Result with a default.
fn result_with_default(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [value, default] = args else {
        return Err("Result.with_default expects value and default".to_string());
    };
    match value {
        ReplValue::Tuple(items) => match items.as_slice() {
            [ReplValue::Atom(tag), value] if tag == "ok" => Ok(value.clone()),
            _ => Ok(default.clone()),
        },
        _ => Ok(default.clone()),
    }
}

/// Parses Unit from String.
fn unit_from_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("Unit.from_string expects String".to_string());
    };
    Ok(if value == "unit" {
        some_value(ReplValue::Unit)
    } else {
        none_value()
    })
}

/// Compares Ordering atoms.
fn ordering_compare(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Atom(left), ReplValue::Atom(right)] = args else {
        return Err("Ordering.compare expects two comparison atoms".to_string());
    };
    let rank = |value: &str| match value {
        "lt" => Some(0),
        "eq" => Some(1),
        "gt" => Some(2),
        _ => None,
    };
    let left = rank(left).ok_or_else(|| "unknown left Ordering atom".to_string())?;
    let right = rank(right).ok_or_else(|| "unknown right Ordering atom".to_string())?;
    Ok(ordering_atom(left.cmp(&right)))
}

/// Converts an atom payload to String.
fn atom_payload_to_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Atom(value)] = args else {
        return Err("atom to_string expects Atom".to_string());
    };
    Ok(ReplValue::String(value.clone()))
}

/// Parses Ordering from String.
fn ordering_from_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("Ordering.from_string expects String".to_string());
    };
    Ok(match value.as_str() {
        "lt" | "eq" | "gt" => some_value(ReplValue::Atom(value.clone())),
        _ => none_value(),
    })
}

/// Builds a compact FileError value.
fn file_new(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Atom(code), ReplValue::String(message), ReplValue::String(path)] = args else {
        return Err("File.new expects Atom, String, String".to_string());
    };
    Ok(file_error_record(code, message, path))
}

/// Reads a compact FileError field.
fn file_error_field(args: &[ReplValue], index: usize, name: &str) -> Result<ReplValue, String> {
    let [ReplValue::Tuple(fields)] = args else {
        return Err(format!("File.{name} expects FileError"));
    };
    fields
        .get(index)
        .cloned()
        .ok_or_else(|| format!("File.{name} received malformed FileError"))
}

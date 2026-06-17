module phase_constructor_pattern_resolution.

pub constructor Some {
    (value: Dynamic): Dynamic ->
        {:some, value}
}.

pub unwrap(input: Dynamic): Dynamic ->
    case input {
        Some(value) -> value
    }.

use std::io::{Read, Write};

use super::support::{
    exchange_worker_resume, transition_continuation, transition_value, transition_value_count,
};

pub(super) fn resume_transition_success(
    input: &mut impl Write,
    output: &mut impl Read,
    request_id: u64,
    transition: &[u8],
) -> Vec<u8> {
    let values = (0..usize::from(transition_value_count(transition)))
        .map(|index| transition_value(transition, index))
        .collect::<Vec<_>>();
    let (kind, success) = exchange_worker_resume(
        input,
        output,
        request_id,
        transition_continuation(transition),
        &values,
    );
    assert_eq!(kind, 4);
    success
}

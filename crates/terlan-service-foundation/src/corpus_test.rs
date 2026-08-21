use super::*;
use crate::{DisabledSink, InMemorySink, LocalFormat, LocalSink};

#[test]
fn disabled_memory_and_local_adapters_accept_same_corpus() {
    emit_semantic_corpus(&DisabledSink).unwrap();
    let memory = InMemorySink::new(16).unwrap();
    emit_semantic_corpus(&memory).unwrap();
    assert_eq!(memory.snapshot().events, semantic_corpus());

    let local = LocalSink::new(LocalFormat::Json, Vec::<u8>::new());
    emit_semantic_corpus(&local).unwrap();
}

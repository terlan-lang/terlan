//! Tests for bounded managed HTTP operations.

use crate::runtime::native_image::{TvmManagedCollectionDescriptor, TvmManagedLayoutDescriptor};
use std::num::NonZeroUsize;

use super::super::{encode_aggregate_append_value_operation, execute_managed_operation};
use super::{
    encode_cookie_header_operation, encode_response_build_operation,
    encode_response_cookie_jar_operation, encode_response_security_headers_operation,
    ManagedCookieHeaderOperation,
};
use crate::runtime::native_image::managed::{
    encode_aggregate_layout, encode_collection_layout, ActorHeap, ActorId, HeapLimits,
    ManagedAggregate, ManagedAggregateDescriptor, ManagedCollectionDescriptor, ManagedFieldType,
    ManagedFieldValue, ManagedLayoutRegistry, ManagedList, ManagedMap, ManagedStringKeySemantics,
    SemanticTypeId, TvmRef,
};

const RESPONSE: &str = "Named(Response)";
const HEADER: &str = "std.http.Response.Header";
const HEADERS: &str = "std.http.Response.Headers";
const JAR: &str = "Named(Jar)";
const MUTATIONS: &str = "std.http.Cookies.Mutations";
const STRING_MAP: &str = "std.http.Request.StringMap";
const SECURITY: &str = "Named(SecurityHeaders)";

/// Builds one actor heap for managed HTTP operation fixtures.
fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(97).expect("actor"),
        HeapLimits::new(1024 * 1024, 16 * 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

/// Builds the exact aggregate and collection registry used by HTTP operations.
fn registry() -> ManagedLayoutRegistry {
    let string = ManagedFieldType::Reference(semantic("std.core.String"));
    let layouts = [
        ManagedAggregateDescriptor::tuple(
            RESPONSE,
            vec![
                ManagedFieldType::Int,
                ManagedFieldType::Int,
                string,
                ManagedFieldType::Int,
                string,
                ManagedFieldType::Reference(semantic(HEADERS)),
            ],
        )
        .expect("response"),
        ManagedAggregateDescriptor::tuple(HEADER, vec![string, string]).expect("header"),
        ManagedAggregateDescriptor::tuple(
            JAR,
            vec![
                ManagedFieldType::Reference(semantic(STRING_MAP)),
                ManagedFieldType::Reference(semantic(MUTATIONS)),
            ],
        )
        .expect("jar"),
        ManagedAggregateDescriptor::tuple(
            SECURITY,
            vec![
                ManagedFieldType::Bool,
                ManagedFieldType::Int,
                ManagedFieldType::Int,
                ManagedFieldType::Int,
                ManagedFieldType::Bool,
            ],
        )
        .expect("security"),
    ]
    .into_iter()
    .map(|descriptor| TvmManagedLayoutDescriptor {
        semantic_id: descriptor.managed().semantic_id().bytes(),
        encoded_layout: encode_aggregate_layout(&descriptor).expect("encode aggregate"),
    })
    .collect::<Vec<_>>();
    let string = ManagedFieldType::Reference(semantic("std.core.String"));
    let collections = [
        ManagedCollectionDescriptor::map(STRING_MAP, string, string).expect("string map"),
        ManagedCollectionDescriptor::list(MUTATIONS, string).expect("mutations"),
        ManagedCollectionDescriptor::list(HEADERS, ManagedFieldType::Reference(semantic(HEADER)))
            .expect("headers"),
    ]
    .into_iter()
    .map(|descriptor| TvmManagedCollectionDescriptor {
        semantic_id: descriptor.semantic_id().bytes(),
        encoded_layout: encode_collection_layout(&descriptor).expect("encode collection"),
    })
    .collect::<Vec<_>>();
    ManagedLayoutRegistry::from_image(
        &layouts,
        &collections,
        &[
            "Deny".to_string(),
            "NoReferrer".to_string(),
            "SameOrigin".to_string(),
            "StrictOriginWhenCrossOrigin".to_string(),
        ],
    )
    .expect("registry")
}

/// Allocates one empty managed response.
fn response(heap: &mut ActorHeap, layouts: &ManagedLayoutRegistry) -> TvmRef<ManagedAggregate> {
    let headers = layouts
        .collection(semantic(HEADERS))
        .and_then(|collection| collection.list_descriptor())
        .expect("headers");
    let headers = heap
        .list_from_elements(headers, &[])
        .expect("empty headers");
    let body = heap.allocate_string("ok").expect("body");
    let content_type = heap.allocate_string("").expect("content type");
    heap.allocate_aggregate(
        layouts.layouts(semantic(RESPONSE))[0].clone(),
        &[
            ManagedFieldValue::Int(0),
            ManagedFieldValue::Int(0),
            ManagedFieldValue::Reference(body.erase()),
            ManagedFieldValue::Int(200),
            ManagedFieldValue::Reference(content_type.erase()),
            ManagedFieldValue::Reference(headers.erase()),
        ],
    )
    .expect("response")
}

/// Allocates one managed string and returns its callback word.
fn string_word(heap: &mut ActorHeap, value: &str) -> i64 {
    word(heap.allocate_string(value).expect("string"))
}

/// Converts one managed reference into its signed callback word.
fn word<T>(reference: TvmRef<T>) -> i64 {
    i64::from_ne_bytes(reference.encoded_abi_word().to_ne_bytes())
}

/// Converts one callback result into its managed reference.
fn result_ref(result: u64) -> TvmRef<()> {
    TvmRef::from_encoded(NonZeroUsize::new(result as usize).expect("reference"))
}

/// Reads response header pairs into owned public strings.
fn response_headers(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    response: TvmRef<()>,
) -> Vec<(String, String)> {
    let response_layout = layouts.layouts(semantic(RESPONSE))[0].clone();
    let response = heap
        .read_aggregate(response.cast(), &response_layout)
        .expect("response");
    let ManagedFieldValue::Reference(headers) = response.field(5).expect("headers") else {
        panic!("headers must be a reference");
    };
    let list = layouts
        .collection(semantic(HEADERS))
        .and_then(|collection| collection.list_descriptor())
        .expect("header list");
    let header_layout = layouts.layouts(semantic(HEADER))[0].clone();
    heap.list_elements(list, headers.cast::<ManagedList>())
        .expect("headers")
        .into_iter()
        .map(|value| {
            let ManagedFieldValue::Reference(value) = value else {
                panic!("header must be a reference");
            };
            let header = heap
                .read_aggregate(value.cast(), &header_layout)
                .expect("header");
            let read = |index| {
                let ManagedFieldValue::Reference(value) =
                    header.field(index).expect("header field")
                else {
                    panic!("header field must be a string");
                };
                heap.read_string(value.cast()).expect("string").to_string()
            };
            (read(0), read(1))
        })
        .collect()
}

/// Builds one checked semantic identity.
fn semantic(canonical: &str) -> SemanticTypeId {
    SemanticTypeId::from_canonical(canonical).expect("semantic")
}

#[test]
fn maintained_cookie_serialization_round_trips_through_managed_strings() {
    let layouts = registry();
    let mut heap = heap();
    let words = [
        string_word(&mut heap, "session"),
        string_word(&mut heap, "abc123"),
        string_word(&mut heap, "/"),
        1,
        1,
    ];
    let encoded = encode_cookie_header_operation(ManagedCookieHeaderOperation::Set);
    let result = execute_managed_operation(&mut heap, &layouts, &encoded, &words)
        .map(result_ref)
        .expect("serialize cookie");
    assert_eq!(
        heap.read_string(result.cast()),
        Ok("session=abc123; HttpOnly; Secure; Path=/")
    );
    let invalid = [
        string_word(&mut heap, "bad name"),
        string_word(&mut heap, "value"),
        string_word(&mut heap, "/"),
        0,
        0,
    ];
    assert!(execute_managed_operation(&mut heap, &layouts, &encoded, &invalid).is_err());
}

#[test]
fn ordinary_response_build_allocates_uniform_defaults_in_one_operation() {
    let layouts = registry();
    let mut heap = heap();
    let payload = string_word(&mut heap, "body");
    let encoded =
        encode_response_build_operation(semantic(RESPONSE), semantic(HEADERS), 2).expect("build");
    let response = execute_managed_operation(&mut heap, &layouts, &encoded, &[payload, 201])
        .map(result_ref)
        .expect("response");
    let layout = layouts.layouts(semantic(RESPONSE))[0].clone();
    let response = heap
        .read_aggregate(response.cast(), &layout)
        .expect("response view");
    assert_eq!(response.field(0), Ok(ManagedFieldValue::Int(0)));
    assert_eq!(response.field(1), Ok(ManagedFieldValue::Int(2)));
    assert_eq!(response.field(3), Ok(ManagedFieldValue::Int(201)));
    let ManagedFieldValue::Reference(payload) = response.field(2).expect("payload") else {
        panic!("payload reference");
    };
    assert_eq!(heap.read_string(payload.cast()), Ok("body"));
    let ManagedFieldValue::Reference(path) = response.field(4).expect("path") else {
        panic!("path reference");
    };
    assert_eq!(heap.read_string(path.cast()), Ok(""));
    let ManagedFieldValue::Reference(headers) = response.field(5).expect("headers") else {
        panic!("headers reference");
    };
    let descriptor = layouts
        .collection(semantic(HEADERS))
        .and_then(|collection| collection.list_descriptor())
        .expect("headers descriptor");
    assert_eq!(heap.list_length(descriptor, headers.cast()), Ok(0));
}

#[test]
fn cookie_jar_replay_and_security_policy_append_persistent_headers() {
    let layouts = registry();
    let mut heap = heap();
    let response = response(&mut heap, &layouts);
    let map = layouts
        .collection(semantic(STRING_MAP))
        .and_then(|collection| collection.map_descriptor())
        .expect("map");
    let incoming = heap
        .map_from_entries(map, &[], &mut ManagedStringKeySemantics)
        .expect("incoming");
    let mutations = layouts
        .collection(semantic(MUTATIONS))
        .and_then(|collection| collection.list_descriptor())
        .expect("mutations");
    let mutations = heap
        .list_from_elements(mutations, &[])
        .expect("empty mutations");
    let jar = heap
        .allocate_aggregate(
            layouts.layouts(semantic(JAR))[0].clone(),
            &[
                ManagedFieldValue::Reference(incoming.cast::<ManagedMap>().erase()),
                ManagedFieldValue::Reference(mutations.erase()),
            ],
        )
        .expect("jar");
    let append = encode_aggregate_append_value_operation(semantic(JAR), semantic(MUTATIONS), 1)
        .expect("append mutation");
    let cookie = string_word(&mut heap, "session=abc123; Path=/");
    let jar = execute_managed_operation(&mut heap, &layouts, &append, &[word(jar), cookie])
        .map(result_ref)
        .expect("append cookie");
    let replay = encode_response_cookie_jar_operation(
        semantic(RESPONSE),
        semantic(HEADERS),
        semantic(HEADER),
        semantic(JAR),
        semantic(MUTATIONS),
        5,
        1,
    )
    .expect("replay operation");
    let response =
        execute_managed_operation(&mut heap, &layouts, &replay, &[word(response), word(jar)])
            .map(result_ref)
            .expect("replay jar");
    assert_eq!(
        response_headers(&heap, &layouts, response),
        vec![(
            "Set-Cookie".to_string(),
            "session=abc123; Path=/".to_string()
        )]
    );

    let policy = heap
        .allocate_aggregate(
            layouts.layouts(semantic(SECURITY))[0].clone(),
            &[
                ManagedFieldValue::Bool(true),
                ManagedFieldValue::Int(0),
                ManagedFieldValue::Int(1),
                ManagedFieldValue::Int(31_536_000),
                ManagedFieldValue::Bool(true),
            ],
        )
        .expect("policy");
    let security = encode_response_security_headers_operation(
        semantic(RESPONSE),
        semantic(HEADERS),
        semantic(HEADER),
        semantic(SECURITY),
        5,
    )
    .expect("security operation");
    let response = execute_managed_operation(
        &mut heap,
        &layouts,
        &security,
        &[word(response), word(policy)],
    )
    .map(result_ref)
    .expect("apply security");
    let headers = response_headers(&heap, &layouts, response);
    assert_eq!(headers.len(), 5);
    assert_eq!(
        headers.last(),
        Some(&(
            "Strict-Transport-Security".to_string(),
            "max-age=31536000; includeSubDomains".to_string()
        ))
    );
}

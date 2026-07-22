use super::{
    VmHttpByteRange, VmHttpResponseBody, VmHttpStaticAssetTable, VmHttpStaticError,
    VmHttpStaticManifestEntry, VmHttpStreamPlan,
};
use crate::runtime::vm::http::response_wire::write_http1_bytes_response;
use crate::runtime::vm::sse::VmSseEvent;

fn manifest_entry(route_path: &str, package_path: &str, bytes: &[u8]) -> VmHttpStaticManifestEntry {
    VmHttpStaticManifestEntry {
        route_path: route_path.to_string(),
        package_path: package_path.to_string(),
        bytes: bytes.to_vec(),
        content_type: None,
        cache_control: None,
        fingerprint: None,
    }
}

/// Verifies a static asset manifest row normalizes response metadata.
///
/// Inputs: one package-relative CSS asset with no explicit metadata.
/// Output: test passes when route lookup exposes inferred content type and
/// conservative cache policy.
/// Transformation: locks deterministic VM static asset metadata before router
/// integration consumes these entries.
#[test]
fn vm_http_static_table_infers_content_type_and_cache_metadata() {
    let mut table = VmHttpStaticAssetTable::new(1024).expect("table");

    table
        .insert(manifest_entry(
            "/assets/app.css",
            "assets/app.css",
            b"body{}",
        ))
        .expect("insert");

    let asset = table.lookup("/assets/app.css").expect("lookup");
    assert_eq!(asset.route_path(), "/assets/app.css");
    assert_eq!(asset.package_path(), "assets/app.css");
    assert_eq!(asset.content_type(), "text/css; charset=utf-8");
    assert_eq!(asset.bytes(), b"body{}");
    assert_eq!(asset.cache_control(), "no-cache");
    assert_eq!(asset.fingerprint(), None);
}

/// Verifies fingerprinted static assets receive immutable cache defaults.
///
/// Inputs: one JavaScript asset with fingerprint metadata.
/// Output: test passes when content type and immutable cache policy are stable.
/// Transformation: pins the manifest behavior needed by bundled web assets.
#[test]
fn vm_http_static_table_marks_fingerprinted_assets_immutable() {
    let mut table = VmHttpStaticAssetTable::new(1024).expect("table");
    let mut entry = manifest_entry("/assets/app.abc123.js", "assets/app.abc123.js", b"main()");
    entry.fingerprint = Some("abc123".to_string());

    table.insert(entry).expect("insert");

    let asset = table.lookup("/assets/app.abc123.js").expect("lookup");
    assert_eq!(asset.content_type(), "text/javascript; charset=utf-8");
    assert_eq!(asset.fingerprint(), Some("abc123"));
    assert_eq!(asset.cache_control(), "public, max-age=31536000, immutable");
}

/// Verifies explicit content and cache metadata override inferred defaults.
///
/// Inputs: one binary asset with explicit content type and cache-control.
/// Output: test passes when overrides are preserved.
/// Transformation: keeps manifest-driven response metadata explicit.
#[test]
fn vm_http_static_table_preserves_manifest_overrides() {
    let mut table = VmHttpStaticAssetTable::new(1024).expect("table");
    let mut entry = manifest_entry("/downloads/report", "assets/report.bin", b"data");
    entry.content_type = Some("application/x-report".to_string());
    entry.cache_control = Some("private, max-age=60".to_string());

    table.insert(entry).expect("insert");

    let asset = table.lookup("/downloads/report").expect("lookup");
    assert_eq!(asset.content_type(), "application/x-report");
    assert_eq!(asset.cache_control(), "private, max-age=60");
}

/// Verifies manifest insertion rejects unsafe paths and duplicate routes.
///
/// Inputs: invalid route paths, invalid package paths, and duplicate routes.
/// Output: test passes when each case returns a typed error.
/// Transformation: prevents static asset manifests from bypassing route/path
/// safety rules.
#[test]
fn vm_http_static_table_rejects_invalid_manifest_entries() {
    let mut table = VmHttpStaticAssetTable::new(4).expect("table");

    assert_eq!(
        table
            .insert(manifest_entry("assets/app.css", "assets/app.css", b"x"))
            .expect_err("route must start with slash"),
        VmHttpStaticError::InvalidRoute
    );
    assert_eq!(
        table
            .insert(manifest_entry("/../secret", "assets/app.css", b"x"))
            .expect_err("route cannot traverse"),
        VmHttpStaticError::InvalidRoute
    );
    assert_eq!(
        table
            .insert(manifest_entry("/asset", "../secret.txt", b"x"))
            .expect_err("asset cannot traverse"),
        VmHttpStaticError::InvalidAssetPath
    );
    assert_eq!(
        table
            .insert(manifest_entry("/asset", "/tmp/secret.txt", b"x"))
            .expect_err("asset cannot be absolute"),
        VmHttpStaticError::InvalidAssetPath
    );
    assert_eq!(
        table
            .insert(manifest_entry("/big", "assets/big.bin", b"12345"))
            .expect_err("asset too large"),
        VmHttpStaticError::AssetTooLarge
    );

    table
        .insert(manifest_entry("/asset", "assets/app.css", b"x"))
        .expect("insert");
    assert_eq!(
        table
            .insert(manifest_entry("/asset", "assets/other.css", b"y"))
            .expect_err("duplicate"),
        VmHttpStaticError::DuplicateRoute
    );
    assert_eq!(
        table.lookup("/missing").expect_err("missing"),
        VmHttpStaticError::AssetNotFound
    );
}

/// Verifies manifest batch insertion is atomic.
///
/// Inputs: one valid entry followed by one duplicate route.
/// Output: test passes when the table rolls back the valid entry from the
/// failed batch.
/// Transformation: ensures malformed manifests do not partially publish static
/// asset state.
#[test]
fn vm_http_static_table_rolls_back_failed_manifest_batch() {
    let mut table = VmHttpStaticAssetTable::new(1024).expect("table");
    let entries = vec![
        manifest_entry("/one", "assets/one.txt", b"one"),
        manifest_entry("/one", "assets/two.txt", b"two"),
    ];

    assert_eq!(
        table
            .insert_manifest(entries)
            .expect_err("duplicate in batch"),
        VmHttpStaticError::DuplicateRoute
    );
    assert_eq!(table.len(), 0);
    assert_eq!(
        table.lookup("/one").expect_err("rolled back"),
        VmHttpStaticError::AssetNotFound
    );
}

/// Verifies response body modes are explicit and typed.
///
/// Inputs: empty, text, binary, static asset, and stream body values.
/// Output: test passes when each mode remains distinguishable.
/// Transformation: prevents implicit conversion from arbitrary values into
/// serialized HTTP bodies.
#[test]
fn vm_http_response_body_modes_are_explicit() {
    let mut table = VmHttpStaticAssetTable::new(1024).expect("table");
    table
        .insert(manifest_entry(
            "/index.html",
            "public/index.html",
            b"<h1>ok</h1>",
        ))
        .expect("insert");
    let asset = table.lookup("/index.html").expect("lookup").clone();
    let stream = VmHttpStreamPlan::new(1024, 4).expect("stream plan");

    assert_eq!(VmHttpResponseBody::Empty, VmHttpResponseBody::Empty);
    assert_eq!(
        VmHttpResponseBody::Text("ok".to_string()),
        VmHttpResponseBody::Text("ok".to_string())
    );
    assert_eq!(
        VmHttpResponseBody::Binary(vec![1, 2, 3]),
        VmHttpResponseBody::Binary(vec![1, 2, 3])
    );
    assert_eq!(
        VmHttpResponseBody::StaticAsset(asset.clone()),
        VmHttpResponseBody::StaticAsset(asset)
    );
    assert_eq!(
        VmHttpResponseBody::SseEventStream(vec![VmSseEvent::data("ok")]),
        VmHttpResponseBody::SseEventStream(vec![VmSseEvent::data("ok")])
    );
    assert_eq!(
        VmHttpResponseBody::Stream(stream.clone()),
        VmHttpResponseBody::Stream(stream)
    );
}

/// Verifies stream plans require explicit non-zero pressure limits.
///
/// Inputs: zero chunk size, zero pending writes, and a valid stream plan.
/// Output: test passes when invalid limits are rejected and valid limits are
/// inspectable.
/// Transformation: keeps response streaming bounded before scheduler-backed
/// stream emission exists.
#[test]
fn vm_http_stream_plan_requires_bounded_nonzero_limits() {
    assert_eq!(
        VmHttpStreamPlan::new(0, 1).expect_err("zero chunk"),
        VmHttpStaticError::InvalidStreamLimit
    );
    assert_eq!(
        VmHttpStreamPlan::new(1, 0).expect_err("zero pending"),
        VmHttpStaticError::InvalidStreamLimit
    );

    let plan = VmHttpStreamPlan::new(4096, 8).expect("valid plan");
    assert_eq!(plan.chunk_size(), 4096);
    assert_eq!(plan.max_pending_writes(), 8);
    assert_eq!(
        VmHttpStreamPlan::unsupported_backend(),
        VmHttpStaticError::UnsupportedStreaming
    );
}

/// Verifies explicit text and binary bodies produce deterministic HTTP bytes.
///
/// Inputs: text and binary response body modes.
/// Output: test passes when status, content headers, length, and body bytes
/// serialize through the VM HTTP/1 writer.
/// Transformation: pins the byte-response adapter used before handler routing
/// can emit static and binary responses directly.
#[test]
fn vm_http_response_body_converts_text_and_binary_to_http_bytes() {
    let text = VmHttpResponseBody::Text("hello".to_string())
        .into_http_response(::http::StatusCode::CREATED)
        .expect("text response");
    let binary = VmHttpResponseBody::Binary(vec![0, 1, 2])
        .into_http_response(::http::StatusCode::OK)
        .expect("binary response");

    assert_eq!(text.status(), ::http::StatusCode::CREATED);
    assert_eq!(text.body(), b"hello");
    assert_eq!(
        header(&text, ::http::header::CONTENT_TYPE),
        "text/plain; charset=utf-8"
    );
    assert_eq!(header(&text, ::http::header::CONTENT_LENGTH), "5");
    assert_eq!(binary.body(), &[0, 1, 2]);
    assert_eq!(
        header(&binary, ::http::header::CONTENT_TYPE),
        "application/octet-stream"
    );
    assert_eq!(header(&binary, ::http::header::CONTENT_LENGTH), "3");

    let mut wire = Vec::new();
    write_http1_bytes_response(&mut wire, &binary, false).expect("write binary response");
    assert!(wire.ends_with(&[0, 1, 2]));
    assert!(String::from_utf8_lossy(&wire).contains("Content-Length: 3\r\n"));
}

/// Verifies static asset bodies carry manifest metadata to HTTP serialization.
///
/// Inputs: fingerprinted JavaScript asset response body.
/// Output: test passes when content type, cache-control, status, and exact
/// bytes are emitted.
/// Transformation: connects manifest normalization to the maintained HTTP
/// response writer without touching package file loading yet.
#[test]
fn vm_http_response_body_converts_static_asset_to_http_bytes() {
    let mut table = VmHttpStaticAssetTable::new(1024).expect("table");
    let mut entry = manifest_entry("/assets/app.abc123.js", "assets/app.abc123.js", b"main()");
    entry.fingerprint = Some("abc123".to_string());
    table.insert(entry).expect("insert");
    let asset = table
        .lookup("/assets/app.abc123.js")
        .expect("lookup")
        .clone();

    let response = VmHttpResponseBody::StaticAsset(asset)
        .into_http_response(::http::StatusCode::OK)
        .expect("static response");

    assert_eq!(response.body(), b"main()");
    assert_eq!(
        header(&response, ::http::header::CONTENT_TYPE),
        "text/javascript; charset=utf-8"
    );
    assert_eq!(
        header(&response, ::http::header::CACHE_CONTROL),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(header(&response, ::http::header::CONTENT_LENGTH), "6");

    let mut wire = Vec::new();
    write_http1_bytes_response(&mut wire, &response, true).expect("write static response");
    let wire_text = String::from_utf8_lossy(&wire);
    assert!(wire_text.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(wire_text.contains("Connection: close\r\n"));
    assert!(wire_text.contains("cache-control: public, max-age=31536000, immutable\r\n"));
    assert!(wire.ends_with(b"main()"));
}

#[test]
fn vm_http_static_asset_emits_typed_byte_range_responses() {
    let mut table = VmHttpStaticAssetTable::new(1024).expect("table");
    table
        .insert(manifest_entry(
            "/asset.txt",
            "assets/asset.txt",
            b"0123456789",
        ))
        .expect("insert");
    let asset = table.lookup("/asset.txt").expect("lookup");

    let inclusive = table
        .range_http_response(
            "/asset.txt",
            VmHttpByteRange::inclusive(2, 5).expect("range"),
        )
        .expect("inclusive response");
    assert_eq!(inclusive.status(), ::http::StatusCode::PARTIAL_CONTENT);
    assert_eq!(inclusive.body(), b"2345");
    assert_eq!(header(&inclusive, ::http::header::ACCEPT_RANGES), "bytes");
    assert_eq!(
        header(&inclusive, ::http::header::CONTENT_RANGE),
        "bytes 2-5/10"
    );
    assert_eq!(header(&inclusive, ::http::header::CONTENT_LENGTH), "4");

    let from = asset
        .range_http_response(VmHttpByteRange::from(7))
        .expect("open response");
    assert_eq!(from.body(), b"789");
    assert_eq!(header(&from, ::http::header::CONTENT_RANGE), "bytes 7-9/10");

    let suffix = asset
        .range_http_response(VmHttpByteRange::suffix(3).expect("suffix"))
        .expect("suffix response");
    assert_eq!(suffix.body(), b"789");
    assert_eq!(
        header(&suffix, ::http::header::CONTENT_RANGE),
        "bytes 7-9/10"
    );
}

#[test]
fn vm_http_static_asset_clamps_and_rejects_adversarial_ranges() {
    let asset = static_asset(b"0123456789");
    let clamped = asset
        .range_http_response(VmHttpByteRange::inclusive(8, usize::MAX).expect("range"))
        .expect("clamped response");
    assert_eq!(clamped.body(), b"89");
    assert_eq!(
        header(&clamped, ::http::header::CONTENT_RANGE),
        "bytes 8-9/10"
    );

    assert_eq!(
        VmHttpByteRange::inclusive(4, 3).expect_err("reversed range"),
        VmHttpStaticError::InvalidRange
    );
    assert_eq!(
        VmHttpByteRange::suffix(0).expect_err("empty suffix"),
        VmHttpStaticError::InvalidRange
    );
    assert_eq!(
        asset
            .range_http_response(VmHttpByteRange::from(10))
            .expect_err("start at length"),
        VmHttpStaticError::UnsatisfiableRange
    );
    assert_eq!(
        static_asset(b"")
            .range_http_response(VmHttpByteRange::suffix(1).expect("suffix"))
            .expect_err("empty asset"),
        VmHttpStaticError::UnsatisfiableRange
    );
}

/// Verifies queued SSE event streams serialize as `text/event-stream`.
///
/// Inputs: two prebuilt SSE event envelopes.
/// Output: test passes when content type, cache policy, length, and exact SSE
/// frame bytes are emitted through the maintained HTTP/1 writer.
/// Transformation: supports deterministic snapshot event streams while live
/// scheduler-backed streaming remains a separate `Stream` contract.
#[test]
fn vm_http_response_body_converts_sse_events_to_http_bytes() {
    let events = vec![
        VmSseEvent::data("one").with_event("counter"),
        VmSseEvent::data("two").with_id("2").with_retry_ms(1500),
    ];
    let response = VmHttpResponseBody::SseEventStream(events)
        .into_http_response(::http::StatusCode::OK)
        .expect("sse response");

    assert_eq!(
        response.body(),
        b"event: counter\ndata: one\n\nid: 2\nretry: 1500\ndata: two\n\n"
    );
    assert_eq!(
        header(&response, ::http::header::CONTENT_TYPE),
        "text/event-stream; charset=utf-8"
    );
    assert_eq!(header(&response, ::http::header::CACHE_CONTROL), "no-cache");
    assert_eq!(header(&response, ::http::header::CONTENT_LENGTH), "55");

    let mut wire = Vec::new();
    write_http1_bytes_response(&mut wire, &response, false).expect("write sse response");
    let wire_text = String::from_utf8_lossy(&wire);
    assert!(wire_text.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(wire_text.contains("content-type: text/event-stream; charset=utf-8\r\n"));
    assert!(wire.ends_with(b"data: two\n\n"));
}

/// Verifies malformed SSE envelopes do not become HTTP responses.
///
/// Inputs: one SSE event with invalid metadata.
/// Output: test passes when response conversion returns a typed static response
/// error before emitting bytes.
/// Transformation: keeps SSE metadata validation inside the VM-owned encoder.
#[test]
fn vm_http_response_body_rejects_invalid_sse_event_stream() {
    assert_eq!(
        VmHttpResponseBody::SseEventStream(vec![VmSseEvent::data("ok").with_event("bad\nevent")])
            .into_http_response(::http::StatusCode::OK)
            .expect_err("invalid event"),
        VmHttpStaticError::InvalidSseEvent
    );
}

/// Verifies stream bodies stay explicitly unsupported until scheduler emission.
///
/// Inputs: valid stream plan body.
/// Output: test passes when conversion returns the typed unsupported-streaming
/// error instead of silently buffering or discarding the stream.
/// Transformation: keeps the current backend boundary honest while static and
/// binary response paths are already serializable.
#[test]
fn vm_http_response_body_rejects_stream_conversion_until_emitter_exists() {
    let stream = VmHttpStreamPlan::new(1024, 2).expect("stream");

    assert_eq!(
        VmHttpResponseBody::Stream(stream)
            .into_http_response(::http::StatusCode::OK)
            .expect_err("stream unsupported"),
        VmHttpStaticError::UnsupportedStreaming
    );
}

fn header(response: &::http::Response<Vec<u8>>, name: ::http::header::HeaderName) -> &str {
    response
        .headers()
        .get(name)
        .expect("header exists")
        .to_str()
        .expect("header text")
}

fn static_asset(bytes: &[u8]) -> super::VmHttpStaticAsset {
    let mut table = VmHttpStaticAssetTable::new(1024).expect("table");
    table
        .insert(manifest_entry("/asset.txt", "assets/asset.txt", bytes))
        .expect("insert");
    table.lookup("/asset.txt").expect("lookup").clone()
}

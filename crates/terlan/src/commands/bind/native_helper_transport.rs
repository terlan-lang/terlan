//! One transport implementation embedded in both C and C++ helpers.

pub(super) const SOURCE: &str = include_str!("../../runtime/vm/package_native_helper/transport.rs");

pub(super) const MAIN: &str = r#"
mod adapter_transport {
@TRANSPORT_SOURCE@
}

fn main() {
    let mut worker = match Worker::new() {
        Ok(worker) => worker,
        Err(error) => {
            eprintln!("native worker identity initialization failed: {error}");
            return;
        }
    };
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut stdout = io::stdout().lock();
    let limits = adapter_transport::Limits {
        frame: MAX_ADAPTER_FRAME_BYTES,
        transfer: MAX_ADAPTER_TRANSFER_BYTES,
    };
    loop {
        let (payload, terminate) = match adapter_transport::read_message(
            &mut input, adapter_transport::Direction::Call, None, limits,
        ) {
            Ok(None) => break,
            Ok(Some(request)) => (worker.execute_line(&request), false),
            Err(error) => {
                let message = error.to_string();
                let code = if message.contains("frame exceeds") {
                    "frame_too_large"
                } else {
                    "invalid_transfer_chunk"
                };
                (protocol_error(code, &message), true)
            }
        };
        if write_response(&mut stdout, &payload, limits).is_err() || terminate {
            break;
        }
    }
}

fn write_response(
    output: &mut impl Write,
    response: &str,
    limits: adapter_transport::Limits,
) -> io::Result<()> {
    let request_id = response.split_whitespace().nth(1)
        .and_then(|field| field.parse::<u64>().ok()).unwrap_or(0);
    let oversized;
    let response = if response.len() > limits.message() {
        oversized = format!("reply {request_id} 1 {}", protocol_error(
            "transfer_too_large", "native adapter response exceeds its bound",
        ));
        &oversized
    } else {
        response
    };
    adapter_transport::write_message(
        output, response, request_id, adapter_transport::Direction::Reply, limits,
    )
}
"#;

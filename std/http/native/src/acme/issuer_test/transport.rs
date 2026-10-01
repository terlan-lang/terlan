use bytes::Bytes;
use http::{Request, Response};
use http_body_util::{BodyExt, Full};
use instant_acme::{BytesResponse, Error, HttpClient};
use serde_json::{json, Value};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    Ready,
    PendingOrder,
    InvalidOrderOnLastPoll,
    ReadyAfterPoll,
    PendingCertificate,
    InvalidAuthorization,
    NoHttp01,
    MalformedDirectory,
    RejectedOrder,
    #[cfg(feature = "acme-issuer")]
    SignedCertificate,
}

#[derive(Clone)]
pub struct TestCa {
    scenario: Scenario,
    rejected_path: Option<&'static str>,
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    requests: Vec<String>,
    finalized: bool,
    order_polls: usize,
    certificate: String,
}

impl TestCa {
    pub fn new(scenario: Scenario) -> Self {
        Self {
            scenario,
            rejected_path: None,
            state: Arc::new(Mutex::new(State::default())),
        }
    }

    pub fn requests(&self) -> Vec<String> {
        self.state.lock().unwrap().requests.clone()
    }

    pub fn rejecting(mut self, path: &'static str) -> Self {
        self.rejected_path = Some(path);
        self
    }
}

fn order(status: &str) -> Value {
    json!({"status": status, "authorizations": ["https://ca.test/auth"],
        "finalize": "https://ca.test/finalize", "certificate": "https://ca.test/certificate"})
}

impl HttpClient for TestCa {
    fn request(
        &self,
        req: Request<Full<Bytes>>,
    ) -> Pin<Box<dyn Future<Output = Result<BytesResponse, Error>> + Send>> {
        let this = self.clone();
        Box::pin(async move {
            let path = req.uri().path().to_string();
            let body = req.into_body().collect().await.unwrap().to_bytes();
            let mut state = this.state.lock().unwrap();
            state.requests.push(path.clone());
            if this.rejected_path == Some(path.as_str()) {
                return Err(Error::Str("test transport rejected request"));
            }
            let mut status = 200;
            let mut location = "https://ca.test/order";
            let value = match path.as_str() {
                "/directory" if this.scenario == Scenario::MalformedDirectory => json!({}),
                "/directory" => json!({"newNonce": "https://ca.test/nonce",
                    "newAccount": "https://ca.test/account", "newOrder": "https://ca.test/new-order"}),
                "/nonce" => json!({}),
                "/account" => {
                    status = 201;
                    location = "https://ca.test/account/1";
                    json!({})
                }
                "/new-order" if this.scenario == Scenario::RejectedOrder => {
                    status = 400;
                    json!({"type": "urn:ietf:params:acme:error:rejectedIdentifier", "detail": "denied"})
                }
                "/new-order" => order(
                    if matches!(
                        this.scenario,
                        Scenario::PendingOrder
                            | Scenario::InvalidOrderOnLastPoll
                            | Scenario::ReadyAfterPoll
                    ) {
                        "pending"
                    } else {
                        "ready"
                    },
                ),
                "/auth" => json!({"identifier": {"type":"dns", "value":"example.test"},
                    "status": if this.scenario == Scenario::InvalidAuthorization {"invalid"} else {"pending"},
                    "challenges": [{"type": if this.scenario == Scenario::NoHttp01 {"dns-01"} else {"http-01"},
                        "url":"https://ca.test/challenge", "token":"test_token", "status":"pending"}]}),
                "/challenge" => json!({"type":"http-01", "url":"https://ca.test/challenge",
                    "token":"test_token", "status":"valid"}),
                "/finalize" => {
                    #[cfg(feature = "acme-issuer")]
                    if this.scenario == Scenario::SignedCertificate {
                        state.certificate = sign_csr(&body);
                    }
                    #[cfg(not(feature = "acme-issuer"))]
                    let _ = body;
                    state.finalized = true;
                    order("processing")
                }
                "/order" => {
                    state.order_polls += 1;
                    order(if state.finalized {
                        if this.scenario == Scenario::PendingCertificate {
                            "processing"
                        } else {
                            "valid"
                        }
                    } else if this.scenario == Scenario::ReadyAfterPoll {
                        "ready"
                    } else if this.scenario == Scenario::InvalidOrderOnLastPoll
                        && state.order_polls == 5
                    {
                        "invalid"
                    } else {
                        "pending"
                    })
                }
                "/certificate" => {
                    return Ok(Response::builder()
                        .body(Full::new(Bytes::from(state.certificate.clone())))
                        .unwrap()
                        .into());
                }
                _ => panic!("unexpected ACME path: {path}"),
            };
            Ok(Response::builder()
                .status(status)
                .header("replay-nonce", "test_nonce")
                .header("location", location)
                .body(Full::new(Bytes::from(value.to_string())))
                .unwrap()
                .into())
        })
    }
}

#[cfg(feature = "acme-issuer")]
fn sign_csr(body: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair};
    use std::{fs, process::Command};
    let jose: Value = serde_json::from_slice(body).unwrap();
    let payload: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(jose["payload"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    let csr = URL_SAFE_NO_PAD
        .decode(payload["csr"].as_str().unwrap())
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut params = CertificateParams::new(vec![]).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let key = KeyPair::generate().unwrap();
    let ca = params.self_signed(&key).unwrap();
    fs::write(dir.path().join("ca.pem"), ca.pem()).unwrap();
    fs::write(dir.path().join("key.pem"), key.serialize_pem()).unwrap();
    fs::write(dir.path().join("request.csr"), csr).unwrap();
    let output = Command::new("openssl")
        .current_dir(dir.path())
        .args([
            "x509",
            "-req",
            "-inform",
            "DER",
            "-in",
            "request.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "key.pem",
            "-CAcreateserial",
            "-copy_extensions",
            "copy",
            "-days",
            "90",
            "-out",
            "issued.pem",
        ])
        .output()
        .expect("acme-issuer integration tests require OpenSSL as a local test CA");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(dir.path().join("issued.pem")).unwrap()
}

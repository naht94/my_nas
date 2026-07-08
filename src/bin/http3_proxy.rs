//! HTTP/3 (QUIC) 리버스 프록시 — Pingora 0.8은 H3 미지원이라 별도 UDP 리스너로 동작한다.
//! TCP HTTPS 프록시와 동일하게 내부 my_nas(3000) / vite(5173)로 전달한다.

use bytes::{Buf, Bytes};
use futures_util::StreamExt;
use h3::server::RequestStream;
use h3_quinn::Connection as H3QuinnConnection;
use http::{header, Method, Response, StatusCode};
use quinn::Endpoint;
use reqwest::Client;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, warn};

/// HTTP/3 요청 바디 최대 버퍼 (이 이상은 TCP HTTPS 사용 권장).
const MAX_H3_REQUEST_BODY: usize = 64 * 1024 * 1024;

pub fn upstream_port(listen_port: u16) -> u16 {
    let ui_upstream = std::env::var("NAS_UI_UPSTREAM_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000);
    match listen_port {
        48481 => 3000,
        48482 => ui_upstream,
        _ => 3000,
    }
}

pub async fn run_http3_listener(cert_path: String, key_path: String, listen_port: u16) {
    let client = match Client::builder()
        .pool_max_idle_per_host(16)
        .tcp_keepalive(std::time::Duration::from_secs(60))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!(%listen_port, "http3: reqwest client init failed: {e}");
            return;
        }
    };

    let endpoint = match quinn_endpoint(&cert_path, &key_path, listen_port) {
        Ok(ep) => ep,
        Err(e) => {
            warn!(%listen_port, "http3: quinn bind failed: {e}");
            return;
        }
    };

    tracing::info!(%listen_port, "http3: QUIC listener ready");

    while let Some(incoming) = endpoint.accept().await {
        let client = client.clone();
        tokio::spawn(async move {
            let connection = match incoming.await {
                Ok(c) => c,
                Err(e) => {
                    debug!("http3: connection handshake failed: {e}");
                    return;
                }
            };

            let h3_conn = H3QuinnConnection::new(connection);
            let mut h3_server = match h3::server::Connection::new(h3_conn).await {
                Ok(s) => s,
                Err(e) => {
                    debug!("http3: h3 connection init failed: {e}");
                    return;
                }
            };

            while let Ok(Some(resolver)) = h3_server.accept().await {
                let client = client.clone();
                tokio::spawn(async move {
                    match resolver.resolve_request().await {
                        Ok((req, stream)) => {
                            if let Err(e) =
                                proxy_request(req, stream, listen_port, &client).await
                            {
                                debug!("http3: request proxy error: {e}");
                            }
                        }
                        Err(e) => debug!("http3: resolve_request failed: {e}"),
                    }
                });
            }
        });
    }
}

fn quinn_endpoint(
    cert_path: &str,
    key_path: &str,
    listen_port: u16,
) -> Result<Endpoint, Box<dyn std::error::Error + Send + Sync>> {
    let cert_file = &mut BufReader::new(File::open(cert_path)?);
    let key_file = &mut BufReader::new(File::open(key_path)?);
    let cert_chain: Vec<rustls::pki_types::CertificateDer<'static>> = rustls_pemfile::certs(cert_file)
        .map(|c| c.unwrap())
        .collect();
    let key = rustls_pemfile::private_key(key_file)?
        .ok_or("missing private key in PEM file")?;

    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)?;
    config.alpn_protocols = vec![b"h3".to_vec()];

    let quic_crypto = quinn::crypto::rustls::QuicServerConfig::try_from(config)?;
    let server_config = quinn::ServerConfig::with_crypto(Arc::new(quic_crypto));
    let addr: SocketAddr = format!("0.0.0.0:{listen_port}").parse()?;
    Ok(Endpoint::server(server_config, addr)?)
}

fn skip_request_header(name: &header::HeaderName) -> bool {
    name == header::CONNECTION
        || name == header::TRANSFER_ENCODING
        || name == header::UPGRADE
        || name == header::HOST
        || name.as_str().eq_ignore_ascii_case("x-user-id")
}

fn skip_response_header(name: &header::HeaderName) -> bool {
    name == header::CONNECTION
        || name == header::TRANSFER_ENCODING
        || name == header::UPGRADE
        || name.as_str().eq_ignore_ascii_case("keep-alive")
}

async fn read_h3_request_body(
    stream: &mut RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let mut body = Vec::new();
    while let Some(mut chunk) = stream.recv_data().await? {
        let n = chunk.remaining();
        if body.len() + n > MAX_H3_REQUEST_BODY {
            return Err("http3 request body too large".into());
        }
        let start = body.len();
        body.resize(start + n, 0);
        chunk.copy_to_slice(&mut body[start..]);
    }
    Ok(body)
}

async fn proxy_request(
    req: http::Request<()>,
    mut h3_stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    listen_port: u16,
    client: &Client,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let target_port = upstream_port(listen_port);
    let path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let url = format!("http://127.0.0.1:{target_port}{path}");

    let method = req.method().clone();
    let has_body = method != Method::GET && method != Method::HEAD;

    let mut upstream_req = client.request(method, &url);
    for (name, value) in req.headers().iter() {
        if skip_request_header(name) {
            continue;
        }
        upstream_req = upstream_req.header(name, value);
    }

    let upstream_res = if has_body {
        let body = match read_h3_request_body(&mut h3_stream).await {
            Ok(b) => b,
            Err(e) => {
                let status = if e.to_string().contains("too large") {
                    StatusCode::PAYLOAD_TOO_LARGE
                } else {
                    StatusCode::BAD_REQUEST
                };
                send_error(&mut h3_stream, status).await;
                return Err(e);
            }
        };
        match upstream_req.body(body).send().await {
            Ok(r) => r,
            Err(e) => {
                send_error(&mut h3_stream, StatusCode::BAD_GATEWAY).await;
                return Err(e.into());
            }
        }
    } else {
        match upstream_req.send().await {
            Ok(r) => r,
            Err(e) => {
                send_error(&mut h3_stream, StatusCode::BAD_GATEWAY).await;
                return Err(e.into());
            }
        }
    };

    forward_response(upstream_res, &mut h3_stream, listen_port).await
}

async fn forward_response(
    upstream_res: reqwest::Response,
    h3_stream: &mut RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    listen_port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let status = upstream_res.status();
    let external_port = match listen_port {
        48481 => 48484,
        48482 => 48483,
        _ => listen_port,
    };
    let mut resp_builder = Response::builder().status(status);
    resp_builder = resp_builder.header(
        "alt-svc",
        format!("h3=\":{external_port}\"; ma={}", 60 * 60 * 12),
    );
    for (name, value) in upstream_res.headers().iter() {
        if skip_response_header(name) {
            continue;
        }
        resp_builder = resp_builder.header(name, value);
    }

    let resp = match resp_builder.body(()) {
        Ok(r) => r,
        Err(e) => {
            send_error(h3_stream, StatusCode::INTERNAL_SERVER_ERROR).await;
            return Err(e.into());
        }
    };

    if h3_stream.send_response(resp).await.is_err() {
        return Ok(());
    }

    let mut byte_stream = upstream_res.bytes_stream();
    while let Some(chunk) = byte_stream.next().await {
        match chunk {
            Ok(bytes) => {
                if h3_stream.send_data(bytes).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                debug!("http3: upstream response stream error: {e}");
                break;
            }
        }
    }

    let _ = h3_stream.finish().await;
    Ok(())
}

async fn send_error(
    h3_stream: &mut RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    status: StatusCode,
) {
    if let Ok(resp) = Response::builder().status(status).body(()) {
        let _ = h3_stream.send_response(resp).await;
        let _ = h3_stream.finish().await;
    }
}

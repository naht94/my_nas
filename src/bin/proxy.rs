// src/bin/proxy.rs

use async_trait::async_trait;
use pingora::Result;
use pingora::http::ResponseHeader;
use pingora::listeners::tls::TlsSettings;
use pingora::proxy::{ProxyHttp, Session, http_proxy_service};
use pingora::server::{Server, ShutdownWatch};
use pingora::services::background::{BackgroundService, background_service}; // 💡 핵심: 커스텀 서비스 통합용
use pingora::upstreams::peer::HttpPeer;
use std::env;
use std::net::SocketAddr;

use bytes::Buf;
use futures_util::StreamExt;
use http::Response;
use reqwest::Client;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;

fn get_target_port(local_port: u16) -> u16 {
    match local_port {
        48481 => 3000, // WebDAV/API
        48482 => 5173, // Web UI
        _ => 3000,
    }
}

pub struct NasProxy;

#[async_trait]

impl ProxyHttp for NasProxy {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {}
    // 💡 핵심: 어떤 문(Port)으로 들어왔는지 확인하여 목적지(Upstream)를 결정합니다.
    async fn upstream_peer(
        &self,
        session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        // 현재 프록시가 요청을 받은 로컬 주소와 포트를 가져옵니다.
        // let local_addr = session.server_addr().and_then(|addr| addr.as_inet());
        let local_port = session
            .server_addr()
            .and_then(|addr| addr.as_inet())
            .map(|a| a.port())
            .unwrap_or(48482);
        // 공유 라우터 호출
        let target_port = get_target_port(local_port);
        let peer = HttpPeer::new(("127.0.0.1", target_port), false, String::new());
        Ok(Box::new(peer))
    }

    async fn response_filter(
        &self,
        session: &mut Session,
        upstream_response: &mut ResponseHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        if let Some(addr) = session.server_addr().and_then(|addr| addr.as_inet()) {
            let local_port = addr.port();
            let external_port = match local_port {
                48481 => 48484, // 내부 48481은 외부 48484에 연결됨
                48482 => 48483, // 내부 48482는 외부 48483에 연결됨
                _ => local_port,
            };
            let alt_svc_val = format!("h3=\":{}\"; ma={}", external_port, 60 * 60 * 12);
            let _ = upstream_response.insert_header("Alt-Svc", alt_svc_val);
        }
        Ok(())
    }
}

pub struct QuicEngineService {
    pub cert_path: String,
    pub key_path: String,
}

#[async_trait]

impl BackgroundService for QuicEngineService {
    async fn start(&self, mut shutdown: ShutdownWatch) {
        let cp1 = self.cert_path.clone();
        let kp1 = self.key_path.clone();
        let cp2 = self.cert_path.clone();
        let kp2 = self.key_path.clone();
        tokio::spawn(async move {
            start_quic_engine(cp1, kp1, 48481).await;
        });
        tokio::spawn(async move {
            start_quic_engine(cp2, kp2, 48482).await;
        });
        // 💡 서버가 종료(SIGTERM)될 때까지 안전하게 대기
        let _ = shutdown.changed().await;
    }
}

async fn start_quic_engine(cert_path: String, key_path: String, listen_port: u16) {
    let http_client = Client::builder().build().unwrap();
    // TLS 로드 (기본적인 rustls 세팅 - 이전 코드 참고하여 구성)
    let cert_file = &mut BufReader::new(File::open(cert_path).unwrap());
    let key_file = &mut BufReader::new(File::open(key_path).unwrap());
    let cert_chain: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(cert_file)
            .map(|c| c.unwrap())
            .collect();

    let key = rustls_pemfile::private_key(key_file)
        .expect("키 파일을 읽을 수 없습니다.")
        .expect("유효한 개인 키가 아닙니다.");

    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .unwrap();

    config.alpn_protocols = vec![b"h3".to_vec()];

    let quic_crypto = quinn::crypto::rustls::QuicServerConfig::try_from(config).unwrap();
    let server_config = quinn::ServerConfig::with_crypto(Arc::new(quic_crypto));
    let addr: SocketAddr = format!("0.0.0.0:{}", listen_port).parse().unwrap();
    let endpoint = quinn::Endpoint::server(server_config, addr).unwrap();

    println!("🚀 [QUIC Engine] UDP {} 포트 가동 중...", listen_port);

    while let Some(incoming) = endpoint.accept().await {
        let http_client = http_client.clone();

        tokio::spawn(async move {
            if let Ok(connection) = incoming.await {
                let h3_conn = h3_quinn::Connection::new(connection);

                if let Ok(mut h3_server) = h3::server::Connection::new(h3_conn).await {
                    while let Ok(Some(resolver)) = h3_server.accept().await {
                        // 💡 공유 라우터 호출 (내부망 타겟 포트 획득)

                        let target_port = get_target_port(listen_port);
                        let client = http_client.clone();

                        tokio::spawn(async move {
                            if let Ok((req, mut stream)) = resolver.resolve_request().await {
                                let path_and_query = req
                                    .uri()
                                    .path_and_query()
                                    .map(|x| x.as_str())
                                    .unwrap_or("/");

                                let target_url =
                                    format!("http://127.0.0.1:{}{}", target_port, path_and_query);
                                let method = req.method().clone();
                                let headers = req.headers().clone();

                                // 2. 클라이언트로부터 바디(JSON) 읽기
                                let mut request_body = Vec::new();
                                while let Ok(Some(mut chunk)) = stream.recv_data().await {
                                    // Buf 트레이트가 임포트되어 있어야 아래와 같이 남은 바이트를 다 가져올 수 있습니다.
                                    let mut bytes = vec![0u8; chunk.remaining()];
                                    chunk.copy_to_slice(&mut bytes);
                                    request_body.extend_from_slice(&bytes);
                                }

                                // 3. 백엔드로 보낼 요청 구성 (바디와 헤더 포함)
                                let mut proxy_request_builder =
                                    client.request(method, &target_url).body(request_body);

                                for (key, value) in headers.iter() {
                                    proxy_request_builder =
                                        proxy_request_builder.header(key, value);
                                }

                                // 4. 전송
                                if let Ok(res) = proxy_request_builder.send().await {
                                    let mut response_builder =
                                        Response::builder().status(res.status());

                                    for (key, value) in res.headers() {
                                        response_builder = response_builder.header(key, value);
                                    }

                                    if stream
                                        .send_response(response_builder.body(()).unwrap())
                                        .await
                                        .is_ok()
                                    {
                                        let mut byte_stream = res.bytes_stream();
                                        while let Some(Ok(chunk)) = byte_stream.next().await {
                                            let _ = stream.send_data(chunk).await;
                                        }
                                        let _ = stream.finish().await;
                                    }
                                }
                            }
                        });
                    }
                }
            }
        });
    }
}

fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();
    dotenvy::dotenv().ok();
    let mut server = Server::new(None).unwrap();
    server.bootstrap();

    // TLS 설정 (인증서 경로)

    let cert_path = env::var("CERT_PATH").expect("CERT_PATH is not available");
    let key_path = env::var("KEY_PATH").expect("KEY_PATH is not available");
    let mut proxy_service = http_proxy_service(&server.configuration, NasProxy);

    let mut tls_settings_1 = TlsSettings::intermediate(&cert_path, &key_path)
        .expect("인증서 파일을 찾을 수 없습니다. 경로와 권한을 확인하세요.");
    tls_settings_1.enable_h2();
    let mut tls_settings_2 = TlsSettings::intermediate(&cert_path, &key_path)
        .expect("인증서 파일을 찾을 수 없습니다. 경로와 권한을 확인하세요.");
    tls_settings_2.enable_h2();

    // HTTP/3 활성화 (pingora용 QUIC 적용을 위한 설정)
    // tls_settings.enable_h3();
    //
    // 대문 1: WebDAV 및 API용 (공유기의 48484 포트와 매칭) HTTPS 설정
    proxy_service.add_tls_with_settings("0.0.0.0:48481", None, tls_settings_1);
    // h3 지원 확정되면 주석 풀기
    // proxy_service.add_udp_with_settings("0.0.0.0:48481", tls_settings.clone());
    // 대문 2: Web UI(프론트)용 (공유기의 48483 포트와 매칭) HTTPS, QUIC
    proxy_service.add_tls_with_settings("0.0.0.0:48482", None, tls_settings_2);
    // h3 지원 확정되면 주석 풀기
    // proxy_service.add_udp_with_settings("0.0.0.0:48482", tls_settings);
    server.add_service(proxy_service);

    // 커스텀 QUIC 적용

    let quic_engine = QuicEngineService {
        cert_path,
        key_path,
    };

    let quic_background_task = background_service("QUIC_Engine", quic_engine);
    server.add_service(quic_background_task);

    println!("🛡️ NAS 리버스 프록시 (HTTPS + QUIC) 가동 중...");
    println!("📍 [48481 TCP/UDP] -> 127.0.0.1:3000 (WebDAV/API)");
    println!("📍 [48482 TCP/UDP] -> 127.0.0.1:5173 (Web UI)");

    server.run_forever();
}

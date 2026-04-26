// src/bin/proxy.rs

use async_trait::async_trait;
use pingora::Result;
use pingora::listeners::tls::TlsSettings;
use pingora::proxy::{ProxyHttp, Session, http_proxy_service};
use pingora::server::Server;
use pingora::upstreams::peer::HttpPeer;
use std::env;
use std::net::SocketAddr;

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
        let local_addr = session.server_addr().and_then(|addr| addr.as_inet());

        let target_port = match local_addr {
            // 8080 포트로 들어왔다면 (공유기의 48484) -> WebDAV/API (3000)
            Some(addr) if addr.port() == 48481 => 3000,
            // 8081 포트로 들어왔다면 (공유기의 48483) -> Web UI (5173)
            Some(addr) if addr.port() == 48482 => 5173,
            // 그 외의 경우 (기본값으로 백엔드 연결)
            _ => 3000,
        };

        let peer = HttpPeer::new(("127.0.0.1", target_port), false, String::new());
        Ok(Box::new(peer))
    }
}

fn main() {
    let mut server = Server::new(None).unwrap();
    server.bootstrap();

    // TLS 설정 (인증서 경로)
    let cert_path = env::var("CERT_PATH").expect("CERT_PATH is not available");
    let key_path = env::var("KEY_PATH").expect("KEY_PATH is not available");

    let mut proxy_service = http_proxy_service(&server.configuration, NasProxy);

    let tls_settings_1 = TlsSettings::intermediate(cert_path, key_path)
        .expect("인증서 파일을 찾을 수 없습니다. 경로와 권한을 확인하세요.");

    let tls_settings_2 = TlsSettings::intermediate(cert_path, key_path)
        .expect("인증서 파일을 찾을 수 없습니다. 경로와 권한을 확인하세요.");
    // HTTP/3 활성화 (QUIC 적용을 위한 설정)
    // tls_settings.enable_h3();

    // 대문 1: WebDAV 및 API용 (공유기의 48484 포트와 매칭) HTTPS 설정
    proxy_service.add_tls_with_settings("0.0.0.0:48481", None, tls_settings_1);
    // h3 지원 확정되면 주석 풀기
    // proxy_service.add_udp_with_settings("0.0.0.0:48481", tls_settings.clone());

    // 대문 2: Web UI(프론트)용 (공유기의 48483 포트와 매칭) HTTPS, QUIC
    proxy_service.add_tls_with_settings("0.0.0.0:48482", None, tls_settings_2);
    // h3 지원 확정되면 주석 풀기
    // proxy_service.add_udp_with_settings("0.0.0.0:48482", tls_settings);

    server.add_service(proxy_service);

    println!("🛡️ NAS 리버스 프록시 (HTTPS) 가동 중...");
    println!("📍 [48481 TCP] -> 127.0.0.1:3000 (WebDAV/API)");
    println!("📍 [48482 TCP] -> 127.0.0.1:5173 (Web UI)");

    server.run_forever();
}

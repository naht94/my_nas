// src/bin/proxy.rs — HTTPS(TCP/HTTP2) + HTTP/3(QUIC) 리버스 프록시

mod http3_proxy;

use async_trait::async_trait;
use http3_proxy::upstream_port;
use pingora::Result;
use pingora::http::{RequestHeader, ResponseHeader};
use pingora::listeners::tls::TlsSettings;
use pingora::proxy::{ProxyHttp, Session, http_proxy_service};
use pingora::server::{Server, ShutdownWatch};
use pingora::services::background::{BackgroundService, background_service};
use pingora::upstreams::peer::HttpPeer;
use std::env;

pub struct NasProxy;

#[async_trait]
impl ProxyHttp for NasProxy {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {}

    async fn upstream_peer(
        &self,
        session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let local_port = session
            .server_addr()
            .and_then(|addr| addr.as_inet())
            .map(|a| a.port())
            .unwrap_or(48482);
        let target_port = upstream_port(local_port);
        let peer = HttpPeer::new(("127.0.0.1", target_port), false, String::new());
        Ok(Box::new(peer))
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        upstream_request.remove_header("X-User-Id");
        Ok(())
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
                48481 => 48484,
                48482 => 48483,
                _ => local_port,
            };
            let alt_svc_val = format!("h3=\":{external_port}\"; ma={}", 60 * 60 * 12);
            let _ = upstream_response.insert_header("Alt-Svc", alt_svc_val);
        }
        Ok(())
    }
}

pub struct Http3Service {
    pub cert_path: String,
    pub key_path: String,
}

#[async_trait]
impl BackgroundService for Http3Service {
    async fn start(&self, mut shutdown: ShutdownWatch) {
        let cp1 = self.cert_path.clone();
        let kp1 = self.key_path.clone();
        let cp2 = self.cert_path.clone();
        let kp2 = self.key_path.clone();

        tokio::spawn(async move {
            http3_proxy::run_http3_listener(cp1, kp1, 48481).await;
        });
        tokio::spawn(async move {
            http3_proxy::run_http3_listener(cp2, kp2, 48482).await;
        });

        let _ = shutdown.changed().await;
    }
}

fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();
    tracing_subscriber::fmt::try_init().ok();
    dotenvy::dotenv().ok();

    let mut server = Server::new(None).unwrap();
    server.bootstrap();

    let cert_path = env::var("CERT_PATH").expect("CERT_PATH is not available");
    let key_path = env::var("KEY_PATH").expect("KEY_PATH is not available");

    let mut proxy_service = http_proxy_service(&server.configuration, NasProxy);

    let mut tls_api = TlsSettings::intermediate(&cert_path, &key_path)
        .expect("인증서 파일을 찾을 수 없습니다. 경로와 권한을 확인하세요.");
    tls_api.enable_h2();

    let mut tls_ui = TlsSettings::intermediate(&cert_path, &key_path)
        .expect("인증서 파일을 찾을 수 없습니다. 경로와 권한을 확인하세요.");
    tls_ui.enable_h2();

    // TCP: my_nas (내부 48481 → 외부 48484, WebDAV·API 호환)
    proxy_service.add_tls_with_settings("0.0.0.0:48481", None, tls_api);
    // TCP: my_nas 또는 vite --dev (내부 48482 → 외부 48483, 기본 웹 URL)
    proxy_service.add_tls_with_settings("0.0.0.0:48482", None, tls_ui);
    server.add_service(proxy_service);

    // UDP: HTTP/3 (동일 포트, QUIC)
    server.add_service(background_service(
        "HTTP3",
        Http3Service {
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
        },
    ));

    tracing::info!("NAS reverse proxy: HTTPS/HTTP2 (TCP) + HTTP/3 (UDP)");
    let ui_upstream = std::env::var("NAS_UI_UPSTREAM_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(3000);
    tracing::info!("  48481 TCP/UDP -> 127.0.0.1:3000 (my_nas)  [외부 48484]");
    tracing::info!(
        "  48482 TCP/UDP -> 127.0.0.1:{ui_upstream} (my_nas 또는 vite)  [외부 48483 — 기본 웹 URL]"
    );

    server.run_forever();
}

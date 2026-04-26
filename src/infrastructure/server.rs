// src/infrastructure/server.rs
use crate::adapters::inbound::http_controller::{delete, get, post};
use crate::application::service::NasService;
use axum::extract::DefaultBodyLimit;
use axum::{
    Router,
    body::Body,
    http::{Response, Uri, header},
    response::IntoResponse,
};
use dav_server::memls::MemLs;
use rust_embed::RustEmbed;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

// 💡 [WebDAV 추가] 필요한 모듈 임포트
use crate::adapters::inbound::webDav::webdav::NasWebDavAdapter;
use axum::{
    extract::Request,
    routing::{any, options},
};
use dav_server::DavHandler;

// 1. Svelte 빌드 결과물을 포함하는 구조체
#[derive(RustEmbed)]
#[folder = "front/svelte-front/build"]
struct Asset;

pub fn build_server(service: Arc<NasService>, webdav_adapter: NasWebDavAdapter) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any) // 테스트 환경에서는 모든 도메인 허용
        .allow_methods(Any)
        .allow_headers(Any);
    let api_routes = Router::new()
        .merge(get::routes())
        .merge(post::routes())
        .merge(delete::routes())
        .layer(DefaultBodyLimit::disable());

    let dav_handler = Arc::new(
        DavHandler::builder()
            .strip_prefix("/webdav")
            .filesystem(Box::new(webdav_adapter))
            .locksystem(MemLs::new())
            .build_handler(),
    );
    let webdav_route = any({
        let handler = dav_handler.clone();
        move |req: Request| async move { handler.handle(req).await }
    });
    Router::new()
        .route("/", options(|| async { axum::http::StatusCode::OK }))
        .route("/webdav", webdav_route.clone())
        .route("/webdav/", webdav_route.clone())
        .route("/webdav/{*path}", webdav_route)
        .nest("/NAS", api_routes)
        .fallback(static_handler)
        .with_state(service)
        .layer(cors)
}

// 2. 통합 바이너리 서빙 핸들러
async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match Asset::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            // SPA 특성상 못 찾는 경로는 index.html로 돌려줘야 Svelte 내부 라우팅이 작동함
            let index = Asset::get("index.html").expect("index.html missing");
            Response::builder()
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from(index.data))
                .unwrap()
        }
    }
}

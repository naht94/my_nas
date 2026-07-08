// src/infrastructure/server.rs
use crate::adapters::inbound::http_controller::{delete, get, patch, post};
use crate::application::service::NasService;
use crate::infrastructure::webdav_auth::{WebDavCredentials, parse_basic_auth};
use axum::extract::DefaultBodyLimit;
use axum::{
    Router,
    body::Body,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Redirect, Response},
};
use dav_server::body::Body as DavBody;
use dav_server::memls::MemLs;
use rust_embed::RustEmbed;
use std::sync::Arc;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

use crate::adapters::inbound::web_dav::webdav::NasWebDavAdapter;
use axum::{
    extract::Request,
    routing::any,
};
use dav_server::DavHandler;

#[derive(RustEmbed)]
#[folder = "front/svelte-front/build"]
struct Asset;

pub fn build_server(service: Arc<NasService>, webdav_adapter: NasWebDavAdapter) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_methods(AllowMethods::mirror_request())
        .allow_headers(AllowHeaders::mirror_request())
        .allow_credentials(true);

    let api_routes = Router::new()
        .merge(get::routes())
        .merge(post::routes())
        .merge(patch::routes())
        .merge(delete::routes())
        .layer(DefaultBodyLimit::disable());

    let nas_app = Router::new()
        .merge(api_routes)
        .fallback(static_handler);

    let dav_handler = Arc::new(
        DavHandler::builder()
            .strip_prefix("/webdav")
            .filesystem(Box::new(webdav_adapter))
            .locksystem(MemLs::new())
            .build_handler(),
    );

    let webdav_route = any({
        let handler = dav_handler.clone();
        let service = service.clone();
        move |req: Request| {
            let handler = handler.clone();
            let service = service.clone();
            async move { handle_webdav(req, handler, service).await }
        }
    });

    Router::new()
        .route("/", axum::routing::get(|| async { Redirect::permanent("/NAS") }))
        .route("/NAS/", axum::routing::get(serve_nas_index))
        .route("/webdav", webdav_route.clone())
        .route("/webdav/", webdav_route.clone())
        .route("/webdav/{*path}", webdav_route)
        .nest("/NAS", nas_app)
        .with_state(service)
        .layer(cors)
}

async fn handle_webdav(
    req: Request,
    handler: Arc<DavHandler<WebDavCredentials>>,
    service: Arc<NasService>,
) -> http::Response<DavBody> {
    if req.method() == axum::http::Method::OPTIONS {
        return handler
            .handle_guarded(
                req,
                String::new(),
                WebDavCredentials { user_id: 0 },
            )
            .await;
    }

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let Some(auth_header) = auth_header else {
        return unauthorized_response();
    };

    let Some((username, password)) = parse_basic_auth(&auth_header) else {
        return unauthorized_response();
    };

    let user_id = match service.verify_app_password(&username, &password).await {
        Some(id) => id,
        None => return unauthorized_response(),
    };

    handler
        .handle_guarded(req, username, WebDavCredentials { user_id })
        .await
}

fn unauthorized_response() -> http::Response<DavBody> {
    http::Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(
            header::WWW_AUTHENTICATE,
            r#"Basic realm="NAS WebDAV", charset="UTF-8""#,
        )
        .body(DavBody::empty())
        .unwrap()
}

/// SvelteKit 정적 빌드 (base `/NAS`). 네스트 밖 요청은 경로에 `NAS/` 접두사가 붙을 수 있다.
fn asset_path_from_uri(uri: &Uri) -> String {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if let Some(rest) = path.strip_prefix("NAS/") {
        path = rest.to_string();
    } else if path == "NAS" {
        path.clear();
    }
    path = path.trim_start_matches('/').to_string();
    if path.is_empty() {
        "index.html".to_string()
    } else {
        path
    }
}

async fn serve_nas_index() -> impl IntoResponse {
    serve_static_asset("index.html").await
}

async fn serve_static_asset(path: &str) -> Response {
    match Asset::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            let index = Asset::get("index.html").expect("index.html missing");
            Response::builder()
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from(index.data))
                .unwrap()
        }
    }
}

/// SvelteKit 정적 빌드 (base `/NAS`). `/NAS` 네스트 안에서는 `/NAS` 접두사가 제거된 경로로 호출된다.
async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = asset_path_from_uri(&uri);
    serve_static_asset(&path).await
}

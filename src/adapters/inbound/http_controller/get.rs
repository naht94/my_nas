use super::handlers;
use crate::application::service::NasService;
use axum::{Router, routing::get};
use std::sync::Arc;

pub fn routes() -> Router<Arc<NasService>> {
    Router::new()
        .route("/", get(handlers::hello_handler))
        .route("/api/files", get(handlers::list_files_handler))
        .route("/api/files/{id}", get(handlers::download_handler))
        .route(
            "/api/storage/usage",
            get(handlers::get_storage_usage_handler),
        )
}

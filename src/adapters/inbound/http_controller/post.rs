// src/adapters/inbound/http/post.rs

use super::handlers;
use crate::application::service::NasService;
use axum::{Router, routing::post};
use std::sync::Arc;

pub fn routes() -> Router<Arc<NasService>> {
    Router::new()
        .route("/api/upload/{filename}", post(handlers::upload_handler))
        .route("/api/folders", post(handlers::create_folder_handler))
        .route("/api/nas/empty-trash", post(handlers::empty_trash_handler))
        .route(
            "/api/files/download-zip",
            post(handlers::download_zip_handler),
        )
}

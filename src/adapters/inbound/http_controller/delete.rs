use super::handlers;
use crate::application::service::NasService;
use axum::{Router, routing::delete};
use std::sync::Arc;

pub fn routes() -> Router<Arc<NasService>> {
    Router::new()
        .route("/api/files/{id}", delete(handlers::delete_file_handler))
        .route("/api/folders/{id}", delete(handlers::delete_folder_handler))
}

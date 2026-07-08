use super::handlers;
use crate::application::service::NasService;
use axum::{Router, routing::patch};
use std::sync::Arc;

pub fn routes() -> Router<Arc<NasService>> {
    Router::new().route(
        "/api/crews/{crew_id}/settings",
        patch(handlers::update_crew_settings_handler),
    )
    .route("/api/files/{id}", patch(handlers::rename_file_handler))
    .route("/api/folders/{id}", patch(handlers::rename_folder_handler))
}

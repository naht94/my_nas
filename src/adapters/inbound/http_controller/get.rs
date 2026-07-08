use super::handlers;
use crate::application::service::NasService;
use axum::{Router, routing::get};
use std::sync::Arc;

pub fn routes() -> Router<Arc<NasService>> {
    Router::new()
        .route("/api/health", get(handlers::hello_handler))
        .route("/api/audit-logs", get(handlers::list_audit_logs_handler))
        .route("/api/users/me", get(handlers::me_handler))
        .route("/api/users/sessions", get(handlers::list_sessions_handler))
        .route(
            "/api/users/app-passwords",
            get(handlers::list_app_passwords_handler),
        )
        .route("/api/files", get(handlers::list_files_handler))
        .route("/api/files/{id}", get(handlers::download_handler))
        .route("/api/files/{id}/stream", get(handlers::stream_file_handler))
        .route("/api/files/{id}/subtitles", get(handlers::list_subtitles_handler))
        .route("/api/files/{id}/as-vtt", get(handlers::subtitle_vtt_handler))
        .route(
            "/api/storage/usage",
            get(handlers::get_storage_usage_handler),
        )
        .route(
            "/api/crews/webdav-mounts",
            get(handlers::list_webdav_mounts_handler),
        )
        .route("/api/crews/mine", get(handlers::list_my_crews_handler))
        .route("/api/crews/visible", get(handlers::list_visible_crews_handler))
        .route(
            "/api/crews/manageable",
            get(handlers::list_manageable_crews_handler),
        )
        .route(
            "/api/crews/deletable",
            get(handlers::list_deletable_crews_handler),
        )
        .route(
            "/api/crews/{crew_id}/members",
            get(handlers::list_crew_members_handler),
        )
        .route(
            "/api/crews/{crew_id}/settings",
            get(handlers::get_crew_settings_handler),
        )
        .route("/api/folders/access", get(handlers::folder_access_handler))
        .route(
            "/api/crews/discover",
            get(handlers::list_discoverable_crews_handler),
        )
        .route(
            "/api/crews/{crew_id}/guest-view",
            get(handlers::get_crew_guest_view_handler),
        )
        .route("/api/files/search", get(handlers::search_files_handler))
        .route("/api/trash", get(handlers::list_trash_handler))
}

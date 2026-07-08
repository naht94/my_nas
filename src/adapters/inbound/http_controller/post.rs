// src/adapters/inbound/http/post.rs

use super::handlers;
use crate::application::service::NasService;
use axum::{Router, routing::post};
use std::sync::Arc;

pub fn routes() -> Router<Arc<NasService>> {
    Router::new()
        .route("/api/users/register", post(handlers::register_user_handler))
        .route("/api/users/login", post(handlers::login_user_handler))
        .route("/api/users/logout", post(handlers::logout_user_handler))
        .route(
            "/api/users/app-passwords",
            post(handlers::create_app_password_handler),
        )
        .route("/api/upload/{filename}", post(handlers::upload_handler))
        .route("/api/folders", post(handlers::create_folder_handler))
        .route("/api/nas/empty-trash", post(handlers::empty_trash_handler))
        .route(
            "/api/files/download-zip",
            post(handlers::download_zip_handler),
        )
        .route("/api/crews", post(handlers::create_crew_handler))
        .route(
            "/api/crews/{crew_id}/join",
            post(handlers::request_join_crew_handler),
        )
        .route(
            "/api/crews/{crew_id}/invite",
            post(handlers::invite_crew_member_handler),
        )
        .route(
            "/api/crews/{crew_id}/approve",
            post(handlers::approve_crew_member_handler),
        )
        .route(
            "/api/crews/{crew_id}/ban",
            post(handlers::ban_crew_member_handler),
        )
        .route(
            "/api/users/change-password",
            post(handlers::change_password_handler),
        )
        .route(
            "/api/users/sessions/revoke-others",
            post(handlers::revoke_other_sessions_handler),
        )
        .route("/api/trash/restore", post(handlers::restore_trash_handler))
        .route(
            "/api/trash/permanent-delete",
            post(handlers::permanent_delete_trash_handler),
        )
}

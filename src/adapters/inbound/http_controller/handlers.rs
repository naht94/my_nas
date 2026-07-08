// src/adapters/inbound/http/handlers.rs

use crate::application::service::{FirstSt, NasService};
use crate::domain::errors::NasError;
use crate::domain::models::{CrewVisibility, Role, Status};
use crate::infrastructure::rate_limit::client_ip_from_headers;
use crate::infrastructure::stream::range_from_headers;
use crate::infrastructure::stream::file_stream_response;
use axum::extract::Query;
use hyper::{HeaderMap, header};
use serde::Deserialize;
use serde_json::{Value, json};

use super::error_response::AppError;
use super::user_context::{cookie_from_headers, RequiredUserId, UserId, SESSION_COOKIE};
use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

const SESSION_MAX_AGE_SECS: i64 = 14 * 24 * 60 * 60;

/// HttpOnly 세션 쿠키. UI/API가 동일 오리진(/NAS)이면 SameSite=Lax 로 충분하다.
fn session_cookie(token: &str, max_age: i64) -> String {
    format!(
        "{name}={value}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={max_age}",
        name = SESSION_COOKIE,
        value = token,
        max_age = max_age,
    )
}

async fn log_audit(
    service: &NasService,
    headers: &HeaderMap,
    user_id: Option<i64>,
    username: Option<&str>,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
    detail: Option<&str>,
) {
    let ip = client_ip_from_headers(headers);
    service
        .record_audit(
            user_id,
            username,
            action,
            target_type,
            target_id,
            detail,
            Some(&ip),
        )
        .await;
}
use std::sync::Arc;
use tokio_util::io::ReaderStream;

#[derive(Deserialize)]
pub struct ListParams {
    folder_id: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateFolderRequest {
    pub name: String,
    pub parent_id: Option<String>,
}
#[derive(serde::Serialize)]
pub struct EmptyTrashResponse {
    pub message: String,
    pub success: bool,
}
#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}
pub type AppResult<T> = Result<T, AppError>;

pub async fn create_folder_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Json(payload): Json<CreateFolderRequest>,
) -> AppResult<impl IntoResponse> {
    service
        .create_folder(
            Some(&payload.name),
            payload.parent_id.as_deref(),
            user_id,
            None,
            true,
        )
        .await?;
    Ok(StatusCode::CREATED)
}
pub async fn list_files_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Query(params): Query<ListParams>,
) -> AppResult<impl IntoResponse> {
    let items = service.list_files(params.folder_id, user_id).await?;
    Ok(Json(items))
}

// 핸들러는 구체적인 로직을 모르고, Service에게 위임합니다.
pub async fn upload_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Path(filename): Path<String>,
    Query(params): Query<ListParams>,
    headers: HeaderMap,
    body: Body,
) -> AppResult<impl IntoResponse> {
    let expected_size = headers
        .get(header::CONTENT_LENGTH)
        .or_else(|| headers.get("X-file-Size"))
        .and_then(|val| val.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let stream = body.into_data_stream();
    let new_id = service
        .upload_file(&filename, params.folder_id, expected_size, stream, user_id)
        .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": new_id}))))
}

pub async fn get_storage_usage_handler(
    State(service): State<Arc<NasService>>,
) -> impl IntoResponse {
    let (total, available) = service.get_storage_usage();

    Json(json!({
        "total": total,
        "available": available,
        "used": total - available
    }))
}

pub async fn download_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let (file, file_type, filename, size) = service.download_file(&id, user_id).await?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", file_type)
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .header(header::CONTENT_LENGTH, size)
        .body(body)
        .unwrap();

    Ok(response)
}

#[derive(Deserialize)]
pub struct StreamParams {
    #[serde(default, deserialize_with = "deserialize_bool_query")]
    pub inline: bool,
}

/// 쿼리 `inline=true` / `inline=1` 등을 모두 허용한다.
fn deserialize_bool_query<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(serde::de::Error::custom(format!(
            "invalid boolean for inline: {other}"
        ))),
    }
}

pub async fn stream_file_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Path(id): Path<String>,
    Query(params): Query<StreamParams>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let (file, file_type, filename, size) = service.download_file(&id, user_id).await?;
    let range = range_from_headers(&headers, size);
    let response = file_stream_response(file, size, &file_type, &filename, params.inline, range)
        .await
        .map_err(|e| AppError(NasError::Internal(e.to_string())))?;
    Ok(response)
}

pub async fn list_subtitles_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let tracks = service.list_subtitle_tracks(&id, user_id).await?;
    Ok(Json(tracks))
}

pub async fn subtitle_vtt_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let vtt = service.subtitle_as_vtt(&id, user_id).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/vtt; charset=utf-8"),
            (header::CACHE_CONTROL, "private, max-age=3600"),
        ],
        vtt,
    ))
}

pub async fn list_sessions_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let token = cookie_from_headers(&headers, SESSION_COOKIE)
        .ok_or(AppError(NasError::Forbidden("세션이 없습니다.".into())))?;
    let sessions = service.list_my_sessions(user_id, &token).await?;
    Ok(Json(sessions))
}

pub async fn revoke_other_sessions_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let token = cookie_from_headers(&headers, SESSION_COOKIE)
        .ok_or(AppError(NasError::Forbidden("세션이 없습니다.".into())))?;
    let revoked = service.revoke_other_sessions(user_id, &token).await?;
    log_audit(
        &service,
        &headers,
        Some(user_id),
        None,
        "sessions_revoke_others",
        None,
        None,
        Some(&format!("revoked={revoked}")),
    )
    .await;
    Ok(Json(json!({
        "message": format!("다른 기기 세션 {revoked}개를 종료했습니다."),
        "revoked": revoked,
    })))
}

pub async fn download_zip_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Json(file_ids): Json<Vec<String>>,
) -> AppResult<impl IntoResponse> {
    let stream = service.download_files_as_zip(file_ids, user_id).await?;

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"NAS_Export.zip\"",
        )
        .body(Body::from_stream(stream))
        .unwrap();

    Ok(response)
}

pub async fn delete_file_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    service.delete_file(&id, user_id).await?;
    log_audit(
        &service,
        &headers,
        user_id,
        None,
        "file_delete",
        Some("file"),
        Some(&id),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_folder_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    service.delete_folder(&id, user_id).await?;
    log_audit(
        &service,
        &headers,
        user_id,
        None,
        "folder_delete",
        Some("folder"),
        Some(&id),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn empty_trash_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    headers: HeaderMap,
    Query(params): Query<ListParams>,
) -> AppResult<impl IntoResponse> {
    let folder_id = params.folder_id.clone();
    service.empty_trash(Some(user_id), params.folder_id).await?;
    log_audit(
        &service,
        &headers,
        Some(user_id),
        None,
        "trash_empty",
        Some("folder"),
        folder_id.as_deref(),
        None,
    )
    .await;
    Ok((
        StatusCode::OK,
        Json(EmptyTrashResponse {
            message: "휴지통을 성공적으로 비웠습니다.".to_string(),
            success: true,
        }),
    ))
}
pub async fn hello_handler() -> Json<Value> {
    Json(FirstSt::hello())
}

pub async fn login_user_handler(
    State(service): State<Arc<NasService>>,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> AppResult<impl IntoResponse> {
    let client_ip = client_ip_from_headers(&headers);
    service.check_auth_rate_limit(&client_ip)?;

    let label = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(120).collect::<String>());

    let login_result = service
        .login_and_create_session(&payload.username, &payload.password, label.as_deref())
        .await;

    match login_result {
        Ok((token, user_id, username)) => {
            service.record_auth_success(&client_ip);
            log_audit(
                &service,
                &headers,
                Some(user_id),
                Some(&username),
                "login",
                None,
                None,
                None,
            )
            .await;
            let global_status = service
                .global_membership_status(user_id)
                .await?
                .map(|s| s as u8);

            let cookie = session_cookie(&token, SESSION_MAX_AGE_SECS);
            Ok((
                [(header::SET_COOKIE, cookie)],
                Json(json!({
                    "user_id": user_id,
                    "username": username,
                    "message": "로그인되었습니다.",
                    "global_status": global_status,
                })),
            ))
        }
        Err(e) => {
            service.record_auth_failure(&client_ip);
            log_audit(
                &service,
                &headers,
                None,
                Some(&payload.username),
                "login_failed",
                None,
                None,
                None,
            )
            .await;
            Err(AppError(e))
        }
    }
}

pub async fn logout_user_handler(
    State(service): State<Arc<NasService>>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    if let Some(token) = cookie_from_headers(&headers, SESSION_COOKIE) {
        if let Some(user_id) = service.validate_session(&token).await {
            log_audit(
                &service,
                &headers,
                Some(user_id),
                None,
                "logout",
                None,
                None,
                None,
            )
            .await;
        }
        let _ = service.logout_session(&token).await;
    }
    let cookie = session_cookie("", 0);
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(json!({ "message": "로그아웃되었습니다." })),
    ))
}

pub async fn me_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
) -> AppResult<impl IntoResponse> {
    let crews = service.list_my_crews(user_id).await?;
    let global_status = service
        .global_membership_status(user_id)
        .await?
        .map(|s| s as u8);
    Ok(Json(json!({
        "user_id": user_id,
        "crews": crews,
        "global_status": global_status,
    })))
}

#[derive(Deserialize)]
pub struct CreateAppPasswordRequest {
    pub label: Option<String>,
}

pub async fn create_app_password_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    Json(payload): Json<CreateAppPasswordRequest>,
) -> AppResult<impl IntoResponse> {
    let (token, id) = service
        .create_app_password(user_id, payload.label.as_deref())
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "app_password": token,
            "message": "이 비밀번호는 다시 표시되지 않습니다. WebDAV 연결 시 이 값을 사용하세요."
        })),
    ))
}

pub async fn list_app_passwords_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
) -> AppResult<impl IntoResponse> {
    let list = service.list_app_passwords(user_id).await?;
    Ok(Json(list))
}

pub async fn revoke_app_password_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    service.revoke_app_password(user_id, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn webdav_public_base(headers: &HeaderMap) -> String {
    if let Ok(base) = std::env::var("WEBDAV_PUBLIC_BASE") {
        return base.trim_end_matches('/').to_string();
    }

    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:3000");
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
        .unwrap_or(if host.starts_with("localhost") || host.starts_with("127.0.0.1") {
            "http"
        } else {
            "https"
        });
    format!("{}://{}/webdav", scheme, host)
}

pub async fn list_webdav_mounts_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let mounts = service
        .list_webdav_mounts(user_id, &webdav_public_base(&headers))
        .await?;
    Ok(Json(mounts))
}

pub async fn register_user_handler(
    State(service): State<Arc<NasService>>,
    headers: HeaderMap,
    Json(payload): Json<RegisterRequest>,
) -> AppResult<impl IntoResponse> {
    let client_ip = client_ip_from_headers(&headers);
    service.check_auth_rate_limit(&client_ip)?;

    let register_result = async {
        let user_id = service
            .register_new_user(&payload.username, &payload.password)
            .await
            .map_err(|e| {
                tracing::error!("회원가입 실패: {:?}", e);
                match e {
                    NasError::BadRequest(_) | NasError::Forbidden(_) => AppError(e),
                    NasError::Internal(msg) if msg.contains("UNIQUE") => AppError(
                        NasError::BadRequest("이미 사용 중인 아이디입니다.".into()),
                    ),
                    other => AppError(other),
                }
            })?;

        let global_status = service
            .global_membership_status(user_id)
            .await?
            .map(|s| s as u8);

        let message = if global_status == Some(Status::Pending as u8) {
            "가입 신청이 접수되었습니다. 글로벌 크루 관리자의 승인 후 이용할 수 있습니다."
        } else {
            "회원가입이 완료되었습니다."
        };

        Ok::<_, AppError>((user_id, global_status, message))
    }
    .await;

    match register_result {
        Ok((user_id, global_status, message)) => {
            service.record_auth_success(&client_ip);
            Ok((
                StatusCode::CREATED,
                Json(json!({
                    "message": message,
                    "user_id": user_id,
                    "global_status": global_status,
                })),
            ))
        }
        Err(e) => {
            service.record_auth_failure(&client_ip);
            Err(e)
        }
    }
}

#[derive(Deserialize)]
pub struct CreateCrewRequest {
    pub parent_crew_id: String,
    pub name: String,
    pub visibility: Option<String>,
    pub max_sub_crew_depth: Option<i32>,
}

#[derive(Deserialize)]
pub struct UpdateCrewSettingsRequest {
    pub max_sub_crew_depth: Option<i32>,
    pub visibility: Option<String>,
}

#[derive(Deserialize)]
pub struct InviteCrewMemberRequest {
    pub target_user_id: Option<i64>,
    pub username: Option<String>,
    pub role: Option<u8>,
}

#[derive(Deserialize)]
pub struct ApproveMemberRequest {
    pub target_user_id: i64,
}

fn parse_visibility(raw: Option<&str>) -> Result<CrewVisibility, AppError> {
    match raw.unwrap_or("public").to_lowercase().as_str() {
        "private" => Ok(CrewVisibility::Private),
        "public" => Ok(CrewVisibility::Public),
        other => Err(AppError(NasError::BadRequest(format!(
            "알 수 없는 visibility: {other}"
        )))),
    }
}

pub async fn create_crew_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    Json(payload): Json<CreateCrewRequest>,
) -> AppResult<impl IntoResponse> {
    let visibility = parse_visibility(payload.visibility.as_deref())?;
    let crew = service
        .create_crew(
            user_id,
            &payload.parent_crew_id,
            &payload.name,
            visibility,
            payload.max_sub_crew_depth,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(crew)))
}

pub async fn update_crew_settings_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    Path(crew_id): Path<String>,
    Json(payload): Json<UpdateCrewSettingsRequest>,
) -> AppResult<impl IntoResponse> {
    let visibility = payload
        .visibility
        .as_deref()
        .map(|v| parse_visibility(Some(v)))
        .transpose()?;
    let crew = service
        .update_crew_settings(user_id, &crew_id, payload.max_sub_crew_depth, visibility)
        .await?;
    Ok(Json(crew))
}

pub async fn list_my_crews_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
) -> AppResult<impl IntoResponse> {
    let crews = service.list_my_crews(user_id).await?;
    Ok(Json(crews))
}

pub async fn list_discoverable_crews_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
) -> AppResult<impl IntoResponse> {
    let crews = service.list_discoverable_crews(user_id).await?;
    Ok(Json(crews))
}

pub async fn list_visible_crews_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
) -> AppResult<impl IntoResponse> {
    let crews = service.list_home_crews(user_id).await?;
    Ok(Json(crews))
}

pub async fn get_crew_guest_view_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Path(crew_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let view = service.get_crew_guest_view(user_id, &crew_id).await?;
    Ok(Json(view))
}

pub async fn request_join_crew_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    Path(crew_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    service.request_join_public_crew(user_id, &crew_id).await?;
    Ok(Json(json!({ "message": "가입 신청이 접수되었습니다." })))
}

pub async fn invite_crew_member_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    Path(crew_id): Path<String>,
    Json(payload): Json<InviteCrewMemberRequest>,
) -> AppResult<impl IntoResponse> {
    let role = payload
        .role
        .and_then(Role::from_u8)
        .unwrap_or(Role::Member);

    if let Some(username) = payload.username.as_deref().filter(|s| !s.trim().is_empty()) {
        service
            .invite_to_crew_by_username(user_id, &crew_id, username, role)
            .await?;
    } else if let Some(target_user_id) = payload.target_user_id {
        service
            .invite_to_crew(user_id, &crew_id, target_user_id, role)
            .await?;
    } else {
        return Err(AppError(NasError::BadRequest(
            "초대할 사용자(아이디 또는 user_id)를 지정해주세요.".into(),
        )));
    }
    Ok(Json(json!({ "message": "초대를 보냈습니다." })))
}

pub async fn get_crew_settings_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    Path(crew_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let crew = service.get_crew_settings(user_id, &crew_id).await?;
    Ok(Json(crew))
}

pub async fn list_manageable_crews_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
) -> AppResult<impl IntoResponse> {
    let crews = service.list_manageable_crews(user_id).await?;
    Ok(Json(crews))
}

pub async fn list_deletable_crews_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
) -> AppResult<impl IntoResponse> {
    let crews = service.list_deletable_crews(user_id).await?;
    Ok(Json(crews))
}

pub async fn list_crew_members_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    Path(crew_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let members = service.list_crew_members(user_id, &crew_id).await?;
    Ok(Json(members))
}

pub async fn folder_access_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Query(params): Query<ListParams>,
) -> AppResult<impl IntoResponse> {
    let (can_read, can_write) = service
        .describe_folder_access(user_id, params.folder_id.as_deref())
        .await;
    Ok(Json(json!({ "can_read": can_read, "can_write": can_write })))
}

pub async fn approve_crew_member_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    headers: HeaderMap,
    Path(crew_id): Path<String>,
    Json(payload): Json<ApproveMemberRequest>,
) -> AppResult<impl IntoResponse> {
    service
        .approve_membership(user_id, &crew_id, payload.target_user_id)
        .await?;
    log_audit(
        &service,
        &headers,
        Some(user_id),
        None,
        "member_approve",
        Some("crew"),
        Some(&crew_id),
        Some(&format!("target_user_id={}", payload.target_user_id)),
    )
    .await;
    Ok(Json(json!({ "message": "멤버십이 승인되었습니다." })))
}

#[derive(Deserialize)]
pub struct RenameFileRequest {
    pub name: Option<String>,
    pub folder_id: Option<String>,
    #[serde(default)]
    pub update_folder_id: bool,
}

#[derive(Deserialize)]
pub struct RenameFolderRequest {
    pub name: Option<String>,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub update_parent_id: bool,
}

pub async fn rename_file_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Path(id): Path<String>,
    Json(payload): Json<RenameFileRequest>,
) -> AppResult<impl IntoResponse> {
    let new_folder = if payload.update_folder_id {
        Some(payload.folder_id.as_deref())
    } else {
        None
    };
    service
        .patch_file(
            &id,
            payload.name.as_deref(),
            new_folder,
            user_id,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn rename_folder_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Path(id): Path<String>,
    Json(payload): Json<RenameFolderRequest>,
) -> AppResult<impl IntoResponse> {
    let new_parent = if payload.update_parent_id {
        Some(payload.parent_id.as_deref())
    } else {
        None
    };
    service
        .patch_folder(
            &id,
            payload.name.as_deref(),
            new_parent,
            user_id,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
}

pub async fn search_files_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Query(params): Query<SearchParams>,
) -> AppResult<impl IntoResponse> {
    let items = service.search_files(user_id, &params.q).await?;
    Ok(Json(items))
}

pub async fn list_trash_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    Query(params): Query<ListParams>,
) -> AppResult<impl IntoResponse> {
    let items = service.list_trash(user_id, params.folder_id).await?;
    Ok(Json(items))
}

#[derive(Deserialize)]
pub struct TrashItemRequest {
    pub id: String,
    pub is_dir: bool,
    pub folder_id: Option<String>,
}

pub async fn restore_trash_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    headers: HeaderMap,
    Json(payload): Json<TrashItemRequest>,
) -> AppResult<impl IntoResponse> {
    service
        .restore_trash_item(user_id, payload.folder_id, &payload.id, payload.is_dir)
        .await?;
    log_audit(
        &service,
        &headers,
        user_id,
        None,
        "trash_restore",
        Some(if payload.is_dir { "folder" } else { "file" }),
        Some(&payload.id),
        None,
    )
    .await;
    Ok(Json(json!({ "message": "복구되었습니다." })))
}

pub async fn permanent_delete_trash_handler(
    State(service): State<Arc<NasService>>,
    UserId(user_id): UserId,
    headers: HeaderMap,
    Json(payload): Json<TrashItemRequest>,
) -> AppResult<impl IntoResponse> {
    service
        .permanent_delete_trash_item(user_id, payload.folder_id, &payload.id, payload.is_dir)
        .await?;
    log_audit(
        &service,
        &headers,
        user_id,
        None,
        "trash_permanent_delete",
        Some(if payload.is_dir { "folder" } else { "file" }),
        Some(&payload.id),
        None,
    )
    .await;
    Ok(Json(json!({ "message": "영구 삭제되었습니다." })))
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

pub async fn change_password_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    headers: HeaderMap,
    Json(payload): Json<ChangePasswordRequest>,
) -> AppResult<impl IntoResponse> {
    service
        .change_password(user_id, &payload.current_password, &payload.new_password)
        .await?;
    log_audit(
        &service,
        &headers,
        Some(user_id),
        None,
        "password_change",
        Some("user"),
        Some(&user_id.to_string()),
        None,
    )
    .await;
    Ok(Json(json!({ "message": "비밀번호가 변경되었습니다." })))
}

#[derive(Deserialize)]
pub struct BanMemberRequest {
    pub target_user_id: i64,
}

pub async fn ban_crew_member_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    headers: HeaderMap,
    Path(crew_id): Path<String>,
    Json(payload): Json<BanMemberRequest>,
) -> AppResult<impl IntoResponse> {
    service
        .ban_crew_member(user_id, &crew_id, payload.target_user_id)
        .await?;
    log_audit(
        &service,
        &headers,
        Some(user_id),
        None,
        "member_ban",
        Some("crew"),
        Some(&crew_id),
        Some(&format!("target_user_id={}", payload.target_user_id)),
    )
    .await;
    Ok(Json(json!({ "message": "멤버가 차단되었습니다." })))
}

pub async fn delete_crew_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    headers: HeaderMap,
    Path(crew_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    service.delete_crew(user_id, &crew_id).await?;
    log_audit(
        &service,
        &headers,
        Some(user_id),
        None,
        "crew_delete",
        Some("crew"),
        Some(&crew_id),
        None,
    )
    .await;
    Ok(Json(json!({ "message": "Crew가 삭제되었습니다." })))
}

#[derive(Deserialize)]
pub struct AuditLogQuery {
    #[serde(default = "default_audit_limit")]
    limit: i32,
}

fn default_audit_limit() -> i32 {
    100
}

pub async fn list_audit_logs_handler(
    State(service): State<Arc<NasService>>,
    RequiredUserId(user_id): RequiredUserId,
    Query(params): Query<AuditLogQuery>,
) -> AppResult<impl IntoResponse> {
    let logs = service.list_audit_logs(user_id, params.limit).await?;
    Ok(Json(logs))
}

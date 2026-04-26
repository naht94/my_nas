// src/adapters/inbound/http/handlers.rs

use crate::application::service::{FirstSt, NasService};
use axum::extract::Query;
use hyper::{HeaderMap, header};
use serde::Deserialize;
use serde_json::{Value, json};

use super::error_response::AppError;
use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
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

pub type AppResult<T> = Result<T, AppError>;

pub async fn create_folder_handler(
    State(service): State<Arc<NasService>>,
    Json(payload): Json<CreateFolderRequest>,
) -> AppResult<impl IntoResponse> {
    service
        .create_folder(Some(&payload.name), payload.parent_id.as_deref())
        .await?;
    Ok(StatusCode::CREATED)
}
pub async fn list_files_handler(
    State(service): State<Arc<NasService>>,
    Query(params): Query<ListParams>,
) -> AppResult<impl IntoResponse> {
    let items = service.list_files(params.folder_id).await?;
    Ok(Json(items))
}

// 핸들러는 구체적인 로직을 모르고, Service에게 위임합니다.
pub async fn upload_handler(
    State(service): State<Arc<NasService>>,
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
        .upload_file(&filename, params.folder_id, expected_size, stream)
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
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let (file, file_type, filename, size) = service.download_file(&id).await?;

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

pub async fn download_zip_handler(
    State(service): State<Arc<NasService>>,
    Json(file_ids): Json<Vec<String>>,
) -> AppResult<impl IntoResponse> {
    let stream = service.download_files_as_zip(file_ids).await?;

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
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    service.delete_file(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_folder_handler(
    State(service): State<Arc<NasService>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    service.delete_folder(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn empty_trash_handler(
    State(service): State<Arc<NasService>>,
) -> AppResult<impl IntoResponse> {
    service.empty_trash().await?;
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

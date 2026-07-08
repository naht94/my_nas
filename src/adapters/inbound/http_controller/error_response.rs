// src/adapters/inbound/http/error.rs

use crate::domain::errors::{NasError, RepoError, StorageError};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

// 1. 도메인 에러를 감싸는 껍데기(Newtype)를 만듭니다.
pub struct AppError(pub NasError);

// 2. ? 연산자를 썼을 때 NasError가 AppError로 자동 변환되도록 From을 구현합니다.
impl From<NasError> for AppError {
    fn from(err: NasError) -> Self {
        AppError(err)
    }
}

// 3. HTTP 응답 매핑은 '이곳(인바운드 어댑터)'에서만 일어납니다!
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("API Error: {:?}", self.0); // 로그는 도메인 에러 기준으로 찍기

        let (status, msg) = match &self.0 {
            // self.0으로 내부 NasError 접근
            NasError::DataNotFound => (StatusCode::NOT_FOUND, "요청한 리소스를 찾을 수 없습니다."),
            NasError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.as_str()),
            NasError::Forbidden(m) => (StatusCode::FORBIDDEN, m.as_str()),
            NasError::Storage(StorageError::NotFound) | NasError::Repo(RepoError::NotFound) => {
                (StatusCode::NOT_FOUND, "파일 또는 폴더가 존재하지 않습니다.")
            }
            NasError::Storage(StorageError::PermissionDenied)
            | NasError::Repo(RepoError::PermissionDenied) => {
                (StatusCode::FORBIDDEN, "접근 권한이 없습니다.")
            }
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "서버 내부 오류가 발생했습니다.",
            ),
        };

        (status, Json(json!({ "error": msg }))).into_response()
    }
}

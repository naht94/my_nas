use crate::application::service::NasService;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts, HeaderMap, StatusCode},
};
use std::sync::Arc;

/// 세션 쿠키 이름.
pub const SESSION_COOKIE: &str = "nas_session";

/// `Cookie` 헤더 문자열에서 특정 쿠키 값을 추출한다.
pub fn cookie_from_headers(headers: &HeaderMap, name: &str) -> Option<String> {
    let header = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in header.split(';') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            if k.trim() == name {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// 요청 Parts에서 세션 쿠키 값을 추출한다.
pub fn cookie_value(parts: &Parts, name: &str) -> Option<String> {
    cookie_from_headers(&parts.headers, name)
}

/// 세션 쿠키를 검증하여 user_id를 담는다. 미인증이면 None.
#[derive(Debug, Clone, Copy)]
pub struct UserId(pub Option<i64>);

impl<S> FromRequestParts<S> for UserId
where
    S: Send + Sync,
    Arc<NasService>: FromRef<S>,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let service = Arc::<NasService>::from_ref(state);
        let user_id = match cookie_value(parts, SESSION_COOKIE) {
            Some(token) => service.validate_session(&token).await,
            None => None,
        };
        Ok(UserId(user_id))
    }
}

/// 인증이 반드시 필요한 핸들러용. 미인증이면 401.
#[derive(Debug, Clone, Copy)]
pub struct RequiredUserId(pub i64);

impl<S> FromRequestParts<S> for RequiredUserId
where
    S: Send + Sync,
    Arc<NasService>: FromRef<S>,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let UserId(user_id) = UserId::from_request_parts(parts, state).await?;
        user_id.map(RequiredUserId).ok_or(StatusCode::UNAUTHORIZED)
    }
}

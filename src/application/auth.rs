use crate::domain::errors::NasError;
use crate::domain::models::{AppPasswordInfo, SessionInfo};
use crate::infrastructure::crypto;
use chrono::{Duration, Utc};
use std::time::{Duration as StdDuration, Instant};
use uuid::Uuid;

use super::service::NasService;

/// 웹 세션 유효기간 (재로그인 주기).
const SESSION_TTL_DAYS: i64 = 14;
/// 토큰 검증 결과를 메모리에 캐시하는 시간. WebDAV 매 요청의 DB 조회를 줄인다.
const TOKEN_CACHE_TTL: StdDuration = StdDuration::from_secs(60);

/// 캐시 키는 토큰 종류를 구분하기 위해 접두사를 붙인다.
fn session_key(hash: &str) -> String {
    format!("s:{hash}")
}
fn app_key(hash: &str) -> String {
    format!("a:{hash}")
}

impl NasService {
    fn cache_get(&self, key: &str) -> Option<i64> {
        let mut cache = self.token_cache.lock().ok()?;
        match cache.get(key) {
            Some((uid, at)) if at.elapsed() < TOKEN_CACHE_TTL => Some(*uid),
            Some(_) => {
                cache.remove(key);
                None
            }
            None => None,
        }
    }

    fn cache_put(&self, key: String, user_id: i64) {
        if let Ok(mut cache) = self.token_cache.lock() {
            cache.insert(key, (user_id, Instant::now()));
        }
    }

    fn cache_remove(&self, key: &str) {
        if let Ok(mut cache) = self.token_cache.lock() {
            cache.remove(key);
        }
    }

    /// 아이디/비밀번호를 검증하고 웹 세션 토큰(평문)을 발급한다.
    /// 반환: (세션 토큰 평문, user_id, username)
    pub async fn login_and_create_session(
        &self,
        username: &str,
        password: &str,
        label: Option<&str>,
    ) -> Result<(String, i64, String), NasError> {
        let (user_id, username) = self.login_user(username, password).await?;

        let token = crypto::generate_token();
        let token_hash = crypto::hash_token(&token);
        let now = Utc::now();
        let expires = now + Duration::days(SESSION_TTL_DAYS);

        self.auth_repository
            .create_session(
                &token_hash,
                user_id,
                label,
                &now.to_rfc3339(),
                &expires.to_rfc3339(),
            )
            .await
            .map_err(|e| NasError::Internal(e.to_string()))?;

        Ok((token, user_id, username))
    }

    /// 세션 토큰(평문)을 검증하여 user_id를 반환한다. 만료/무효면 None.
    pub async fn validate_session(&self, token: &str) -> Option<i64> {
        let token_hash = crypto::hash_token(token);
        let key = session_key(&token_hash);

        if let Some(uid) = self.cache_get(&key) {
            return Some(uid);
        }

        let (user_id, expires_at) = self.auth_repository.find_session(&token_hash).await.ok()??;

        let expired = chrono::DateTime::parse_from_rfc3339(&expires_at)
            .map(|exp| exp <= Utc::now())
            .unwrap_or(true);
        if expired {
            let _ = self.auth_repository.delete_session(&token_hash).await;
            return None;
        }

        self.cache_put(key, user_id);
        Some(user_id)
    }

    /// 세션을 폐기한다 (로그아웃).
    pub async fn logout_session(&self, token: &str) -> Result<(), NasError> {
        let token_hash = crypto::hash_token(token);
        self.cache_remove(&session_key(&token_hash));
        self.auth_repository
            .delete_session(&token_hash)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))
    }

    /// 만료된 세션을 정리한다 (시작 시 1회 호출용).
    pub async fn cleanup_expired_sessions(&self) -> Result<(), NasError> {
        self.auth_repository
            .delete_expired_sessions(&Utc::now().to_rfc3339())
            .await
            .map_err(|e| NasError::Internal(e.to_string()))
    }

    /// WebDAV용 앱 비밀번호를 발급한다. 평문 토큰은 이 시점에만 반환된다.
    /// 반환: (앱 비밀번호 평문, 앱 비밀번호 id)
    pub async fn create_app_password(
        &self,
        user_id: i64,
        label: Option<&str>,
    ) -> Result<(String, String), NasError> {
        if let Some(status) = self.global_membership_status(user_id).await? {
            if !status.is_active() {
                return Err(NasError::Forbidden("가입 승인 대기 중입니다.".into()));
            }
        }

        let token = crypto::generate_token();
        let token_hash = crypto::hash_token(&token);
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        self.auth_repository
            .create_app_password(&id, user_id, &token_hash, label, &now)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))?;

        Ok((token, id))
    }

    /// WebDAV Basic Auth 검증: username + 앱 비밀번호(평문)로 user_id를 확인한다.
    /// 랜덤 토큰이라 빠른 해시(BLAKE3) + 캐시로 매 요청에도 부담이 없다.
    pub async fn verify_app_password(&self, username: &str, secret: &str) -> Option<i64> {
        let token_hash = crypto::hash_token(secret);
        let key = app_key(&token_hash);

        let user_id = if let Some(uid) = self.cache_get(&key) {
            uid
        } else {
            let uid = self
                .auth_repository
                .find_app_password_user(&token_hash)
                .await
                .ok()??;
            self.cache_put(key, uid);
            let _ = self
                .auth_repository
                .touch_app_password(&token_hash, &Utc::now().to_rfc3339())
                .await;
            uid
        };

        // 제공된 username이 토큰 소유자와 일치하는지 확인한다.
        let user = self
            .crew_repository
            .find_user_by_username(username)
            .await
            .ok()??;
        if user.id == user_id {
            Some(user_id)
        } else {
            None
        }
    }

    pub async fn list_app_passwords(
        &self,
        user_id: i64,
    ) -> Result<Vec<AppPasswordInfo>, NasError> {
        self.auth_repository
            .list_app_passwords(user_id)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))
    }

    pub async fn revoke_app_password(&self, user_id: i64, id: &str) -> Result<(), NasError> {
        // 캐시는 해시 키 기반이라 id로 직접 무효화할 수 없으나, TTL(60초) 후 자연 만료된다.
        self.auth_repository
            .delete_app_password(id, user_id)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))
    }

    pub fn check_auth_rate_limit(&self, client_key: &str) -> Result<(), NasError> {
        self.login_rate_limiter.check(client_key)
    }

    pub fn record_auth_failure(&self, client_key: &str) {
        self.login_rate_limiter.record_failure(client_key);
    }

    pub fn record_auth_success(&self, client_key: &str) {
        self.login_rate_limiter.record_success(client_key);
    }

    pub async fn list_my_sessions(
        &self,
        user_id: i64,
        current_token: &str,
    ) -> Result<Vec<SessionInfo>, NasError> {
        let current_hash = crypto::hash_token(current_token);
        let rows = self
            .auth_repository
            .list_session_records(user_id)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(hash, label, created_at, expires_at)| SessionInfo {
                label,
                created_at,
                expires_at,
                is_current: hash == current_hash,
            })
            .collect())
    }

    pub async fn revoke_other_sessions(
        &self,
        user_id: i64,
        current_token: &str,
    ) -> Result<u64, NasError> {
        let current_hash = crypto::hash_token(current_token);
        self.auth_repository
            .delete_other_sessions(user_id, &current_hash)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))
    }
}

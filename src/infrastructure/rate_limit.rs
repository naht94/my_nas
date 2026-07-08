use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::domain::errors::NasError;

const MAX_FAILURES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(15 * 60);
const WINDOW: Duration = Duration::from_secs(15 * 60);

#[derive(Debug)]
struct Entry {
    failures: u32,
    window_start: Instant,
    locked_until: Option<Instant>,
}

/// 로그인·회원가입 등 인증 시도에 대한 IP 단위 제한 (프로세스 메모리).
#[derive(Debug, Default)]
pub struct LoginRateLimiter {
    inner: Mutex<HashMap<String, Entry>>,
}

impl LoginRateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// 차단 중이면 Err. 아니면 Ok.
    pub fn check(&self, key: &str) -> Result<(), NasError> {
        let mut map = self.inner.lock().map_err(|_| {
            NasError::Internal("rate limiter lock poisoned".into())
        })?;
        let now = Instant::now();
        let entry = map.entry(key.to_string()).or_insert(Entry {
            failures: 0,
            window_start: now,
            locked_until: None,
        });

        if let Some(until) = entry.locked_until {
            if now < until {
                let secs = (until - now).as_secs().max(1);
                return Err(NasError::Forbidden(format!(
                    "로그인 시도가 너무 많습니다. {secs}초 후 다시 시도하세요."
                )));
            }
            entry.locked_until = None;
            entry.failures = 0;
            entry.window_start = now;
        }

        if now.duration_since(entry.window_start) > WINDOW {
            entry.failures = 0;
            entry.window_start = now;
        }

        Ok(())
    }

    pub fn record_failure(&self, key: &str) {
        let Ok(mut map) = self.inner.lock() else {
            return;
        };
        let now = Instant::now();
        let entry = map.entry(key.to_string()).or_insert(Entry {
            failures: 0,
            window_start: now,
            locked_until: None,
        });

        if now.duration_since(entry.window_start) > WINDOW {
            entry.failures = 0;
            entry.window_start = now;
        }

        entry.failures += 1;
        if entry.failures >= MAX_FAILURES {
            entry.locked_until = Some(now + LOCKOUT);
        }
    }

    pub fn record_success(&self, key: &str) {
        let Ok(mut map) = self.inner.lock() else {
            return;
        };
        map.remove(key);
    }
}

/// 프록시 뒤에서 클라이언트 IP 추정.
pub fn client_ip_from_headers(headers: &axum::http::HeaderMap) -> String {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            let ip = first.trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
    }
    if let Some(xri) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let ip = xri.trim();
        if !ip.is_empty() {
            return ip.to_string();
        }
    }
    "unknown".to_string()
}

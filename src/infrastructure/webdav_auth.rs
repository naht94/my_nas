use base64::Engine;

/// dav-server `handle_guarded` 로 전달되는 인증 컨텍스트.
/// PROPFIND 스트리밍 등 하위 태스크에서도 user_id 가 유지된다.
#[derive(Clone, Copy, Debug)]
pub struct WebDavCredentials {
    pub user_id: i64,
}

pub fn parse_basic_auth(header_value: &str) -> Option<(String, String)> {
    let encoded = header_value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (username, password) = decoded.split_once(':')?;
    Some((username.to_string(), password.to_string()))
}

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use rand::RngCore;

/// 저전력 NAS(N100/8GB)에 맞춘 Argon2id 파라미터.
/// m=19MiB, t=2, p=1 (OWASP 권장 최소선). 로그인 시점에만 사용.
fn argon2() -> Argon2<'static> {
    let params = Params::new(19 * 1024, 2, 1, None).expect("유효한 Argon2 파라미터");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// 사람이 입력하는 비밀번호를 Argon2id PHC 문자열로 해싱한다.
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

/// 비밀번호를 저장된 PHC 해시와 검증한다. (Argon2id)
pub fn verify_password(password: &str, phc: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    argon2()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// 세션·앱 비밀번호용 고엔트로피 랜덤 토큰(32바이트)을 생성한다.
/// 엔트로피가 충분하므로 저장 시 느린 해시가 필요 없다.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 랜덤 토큰을 빠른 해시(BLAKE3)로 변환한다. 요청 경로에서 사용해도 부담이 없다.
pub fn hash_token(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

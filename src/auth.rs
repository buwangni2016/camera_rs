/*!
 * 认证工具（密码哈希、API Key 验证）
 *
 * 密码存储策略：
 *   - 新密码使用 Argon2id + 随机盐（PHC string 格式）
 *   - 旧的 SHA-256 hex 哈希在首次成功登录时自动升级
 *   - 通过前缀区分：PHC string 以 "$argon2" 开头；SHA-256 hex 为 64 位十六进制
 */

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use sha2::{Digest, Sha256};

/// 使用 Argon2id 对密码进行哈希，返回 PHC string 格式
pub fn hash_password(pw: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .expect("Argon2 hashing failed")
        .to_string()
}

/// 验证密码。支持：
///   1. Argon2id PHC string（新格式，以 "$argon2" 开头）
///   2. SHA-256 hex（旧格式，向后兼容，自动通过调用层升级）
pub fn verify_password(input: &str, stored: &str) -> bool {
    if stored.starts_with("$argon2") {
        // 新格式：Argon2id
        match PasswordHash::new(stored) {
            Ok(parsed) => Argon2::default()
                .verify_password(input.as_bytes(), &parsed)
                .is_ok(),
            Err(_) => false,
        }
    } else {
        // 旧格式：SHA-256（迁移期兼容）
        sha256_hex(input) == stored
    }
}

/// 检查存储的哈希是否仍为旧的 SHA-256 格式（用于触发自动升级）
pub fn needs_upgrade(stored: &str) -> bool {
    !stored.starts_with("$argon2")
}

/// 生成随机 API Key
pub fn generate_api_key() -> String {
    use uuid::Uuid;
    format!("crk_{}", Uuid::new_v4().to_string().replace('-', ""))
}

/// SHA-256 hex（仅供 API Key 哈希或旧密码对比）
pub fn sha256_hex(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2_round_trip() {
        let hash = hash_password("correct_horse_battery");
        assert!(verify_password("correct_horse_battery", &hash));
        assert!(!verify_password("wrong_password", &hash));
    }

    #[test]
    fn argon2_hash_is_phc_format() {
        let hash = hash_password("test");
        assert!(hash.starts_with("$argon2"), "expected PHC format, got: {hash}");
    }

    #[test]
    fn sha256_backward_compat() {
        // 旧系统存储的 SHA-256 hash 仍可验证
        let old_hash = sha256_hex("admin");
        assert!(verify_password("admin", &old_hash));
        assert!(!verify_password("wrong", &old_hash));
    }

    #[test]
    fn needs_upgrade_detection() {
        let old_hash = sha256_hex("admin");
        let new_hash = hash_password("admin");
        assert!(needs_upgrade(&old_hash));
        assert!(!needs_upgrade(&new_hash));
    }

    #[test]
    fn two_hashes_differ() {
        // Argon2id 每次生成不同盐
        let h1 = hash_password("same");
        let h2 = hash_password("same");
        assert_ne!(h1, h2, "每次应生成不同盐");
        assert!(verify_password("same", &h1));
        assert!(verify_password("same", &h2));
    }
}

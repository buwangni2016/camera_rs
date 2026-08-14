/*!
 * 认证工具（密码哈希、API Key 验证）
 */

use sha2::{Digest, Sha256};

pub fn hash_password(pw: &str) -> String {
    let mut h = Sha256::new();
    h.update(pw.as_bytes());
    hex::encode(h.finalize())
}

pub fn verify_password(input: &str, hash: &str) -> bool {
    hash_password(input) == hash
}

/// 生成随机 API Key
pub fn generate_api_key() -> String {
    use uuid::Uuid;
    format!("crk_{}", Uuid::new_v4().to_string().replace('-', ""))
}

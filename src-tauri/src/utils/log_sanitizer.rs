//! 安全日志脱敏工具
//!
//! 提供敏感信息脱敏函数，确保 API Key、Token、密码等不会明文写入日志文件。
//! 用法：
//! ```ignore
//! tracing::info!("API Key: {}", sanitize_key(&api_key));
//! tracing::info!("Token: {}", mask_sensitive(&token, 4));
//! ```

/// 对字符串进行脱敏：仅保留首尾各 `visible` 个字符，中间用 `***` 替代。
/// 如果字符串长度不足 `visible * 2 + 3`，则全部替换为 `***`。
///
/// # 示例
/// ```
/// assert_eq!(mask_sensitive("sk-1234567890abcdef", 4), "sk-1***cdef");
/// assert_eq!(mask_sensitive("short", 4), "***");
/// ```
pub fn mask_sensitive(s: &str, visible: usize) -> String {
    let len = s.chars().count();
    let min_len = visible * 2 + 3; // 至少保留首尾 + "***"
    if len < min_len {
        return "***".to_string();
    }
    let prefix: String = s.chars().take(visible).collect();
    let suffix: String = s.chars().rev().take(visible).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{}***{}", prefix, suffix)
}

/// 对 API Key 脱敏（首4尾4，与 AI 调用日志保持一致）
pub fn sanitize_key(key: &str) -> String {
    mask_sensitive(key, 4)
}

/// 对 Token 脱敏（首4尾4）
pub fn sanitize_token(token: &str) -> String {
    mask_sensitive(token, 4)
}

/// 对文件路径脱敏：仅保留文件名，隐藏完整路径中的用户名
pub fn sanitize_path(path: &str) -> String {
    // 将 Windows 用户名替换为 ***
    let re = regex::Regex::new(r"C:\\Users\\([^\\]+)").unwrap();
    re.replace_all(path, "C:\\Users\\***").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_normal() {
        assert_eq!(
            mask_sensitive("sk-1234567890abcdef", 4),
            "sk-1***cdef"
        );
    }

    #[test]
    fn test_mask_short() {
        assert_eq!(mask_sensitive("abc", 4), "***");
        assert_eq!(mask_sensitive("12345678", 4), "***");
    }

    #[test]
    fn test_mask_exact() {
        // 4+3+4=11 chars minimum
        assert_eq!(
            mask_sensitive("12345678901", 4),
            "1234***8901"
        );
    }

    #[test]
    fn test_sanitize_key() {
        assert_eq!(
            sanitize_key("sk-abcdefghijklmnop"),
            "sk-a***mnop"
        );
    }

    #[test]
    fn test_sanitize_path() {
        let p = r"C:\Users\sandorn\AppData\Roaming\ExcelMiner\config.toml";
        let masked = sanitize_path(p);
        assert!(masked.contains("***"));
        assert!(!masked.contains("sandorn"));
    }
}

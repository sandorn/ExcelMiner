//! 文本数字提取

use std::sync::OnceLock;
use regex::Regex;

/// 从文本中提取数字，处理各种格式：
/// - "直播1736场" → 1736.0
/// - "1+1000" → 1001.0
/// - "¥1,234.56" → 1234.56
/// - "85%" → 0.85
/// - 纯中文/无数字 → None
pub fn extract_number(text: &str) -> Option<f64> {
    if text.is_empty() {
        return None;
    }

    let trimmed = text.trim();

    // 1. 先处理百分比: "85%", "85.5%"
    if let Some(pct) = parse_percent(trimmed) {
        return Some(pct);
    }

    // 2. 处理 "a+b" 表达式格式: "1+1000"
    if let Some(sum) = eval_expression(trimmed) {
        return Some(sum);
    }

    // 3. 提取干净的数值字符串
    let cleaned = clean_number_text(trimmed);
    if cleaned.is_empty() {
        return None;
    }

    // 4. 解析为 f64
    cleaned.parse::<f64>().ok()
}

fn percent_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"([\d,]+\.?\d*)\s*%").unwrap())
}

fn expression_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\d+)\s*\+\s*(\d+)$").unwrap())
}

fn number_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(-?\d+\.?\d*)").unwrap())
}

/// 解析百分比: "85%" → 0.85
fn parse_percent(text: &str) -> Option<f64> {
    let caps = percent_regex().captures(text)?;
    let num_str = caps.get(1)?.as_str().replace(',', "");
    let num = num_str.parse::<f64>().ok()?;
    Some(num / 100.0)
}

/// 处理 "a+b" 格式（a、b 为整数）: "1+1000" → 1001.0
fn eval_expression(text: &str) -> Option<f64> {
    let caps = expression_regex().captures(text.trim())?;
    let a = caps.get(1)?.as_str().parse::<f64>().ok()?;
    let b = caps.get(2)?.as_str().parse::<f64>().ok()?;
    Some(a + b)
}

/// 清理文本：去除货币符号、千分位逗号、中文等
fn clean_number_text(text: &str) -> String {
    // 先去掉常见的货币和单位符号
    let s = text
        .replace('¥', "")
        .replace('￥', "")
        .replace('$', "")
        .replace('€', "")
        .replace('元', "")
        .replace('万', "")
        .replace(',', "");

    // 提取连续的数字部分（含小数点）
    if let Some(caps) = number_regex().captures(&s) {
        return caps.get(1).unwrap().as_str().to_string();
    }

    // 回退：直接过滤
    s.chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_number() {
        assert_eq!(extract_number("直播1736场"), Some(1736.0));
        assert_eq!(extract_number("1+1000"), Some(1001.0));
        assert_eq!(extract_number("¥1,234.56"), Some(1234.56));
        assert_eq!(extract_number("85%"), Some(0.85));
        assert_eq!(extract_number("1234"), Some(1234.0));
        assert_eq!(extract_number(""), None);
        assert_eq!(extract_number("纯中文文本"), None);
        assert_eq!(extract_number("85.5%"), Some(0.855));
        assert_eq!(extract_number("-500"), Some(-500.0));
    }
}

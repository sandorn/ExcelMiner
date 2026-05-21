/// 日期工具函数
use chrono::{Datelike, NaiveDate};

/// 从字符串解析月份
/// 支持格式: "2024年6月", "2024-06", "202406"
pub fn parse_month(text: &str) -> Option<(u32, u32)> {
    let re_year_month = regex::Regex::new(r"(\d{4})\s*年\s*(\d{1,2})\s*月").ok()?;
    let re_dash = regex::Regex::new(r"(\d{4})-(\d{1,2})").ok()?;

    // "2024年6月"
    if let Some(caps) = re_year_month.captures(text) {
        let year: u32 = caps.get(1)?.as_str().parse().ok()?;
        let month: u32 = caps.get(2)?.as_str().parse().ok()?;
        if (1..=12).contains(&month) {
            return Some((year, month));
        }
    }

    // "2024-06"
    if let Some(caps) = re_dash.captures(text) {
        let year: u32 = caps.get(1)?.as_str().parse().ok()?;
        let month: u32 = caps.get(2)?.as_str().parse().ok()?;
        if (1..=12).contains(&month) {
            return Some((year, month));
        }
    }

    None
}

/// 获取 YTD 月份列表：(起始年, 起始月) → [(年, 月), ...]
pub fn ytd_months(year: u32, month: u32, count: u32) -> Vec<(u32, u32)> {
    let mut result = Vec::new();
    for i in 0..count {
        let m_offset = month as i32 - i as i32;
        if m_offset >= 1 {
            result.push((year, m_offset as u32));
        } else {
            result.push((year - 1, (12 + m_offset) as u32));
        }
    }
    result.reverse();
    result
}

/// 从文件夹名解析日期
pub fn parse_date_from_folder(folder_name: &str) -> Option<(u32, u32)> {
    parse_month(folder_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_month() {
        assert_eq!(parse_month("2024年6月"), Some((2024, 6)));
        assert_eq!(parse_month("2024年12月"), Some((2024, 12)));
        assert_eq!(parse_month("2024-06"), Some((2024, 6)));
        assert_eq!(parse_month("abc"), None);
    }

    #[test]
    fn test_ytd_months() {
        let months = ytd_months(2024, 6, 5);
        assert_eq!(months, vec![(2024, 2), (2024, 3), (2024, 4), (2024, 5), (2024, 6)]);

        let months = ytd_months(2024, 2, 3);
        assert_eq!(months, vec![(2023, 12), (2024, 1), (2024, 2)]);
    }
}

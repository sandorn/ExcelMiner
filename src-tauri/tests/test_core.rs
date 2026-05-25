//! 核心工具测试: number_parser, date_utils, quality_checker

use excelminer_lib::services::number_parser::extract_number;
use excelminer_lib::models::analysis::AnalysisQuality;

// ============================================================
// Number Parser 测试
// ============================================================

#[test]
fn test_extract_plain_number() {
    assert_eq!(extract_number("1234"), Some(1234.0));
    assert_eq!(extract_number("0"), Some(0.0));
    assert_eq!(extract_number("-500"), Some(-500.0));
}

#[test]
fn test_extract_from_chinese_text() {
    // "直播1736场" → 1736
    assert_eq!(extract_number("直播1736场"), Some(1736.0));
    // "达成率85%"
    assert_eq!(extract_number("达成率85%"), Some(0.85));
    // 纯中文
    assert_eq!(extract_number("纯中文文本"), None);
}

#[test]
fn test_extract_with_plus_expr() {
    // "1+1000" → 1001
    assert_eq!(extract_number("1+1000"), Some(1001.0));
    // "0+500" → 500
    assert_eq!(extract_number("0+500"), Some(500.0));
}

#[test]
fn test_extract_with_currency() {
    // "¥1,234.56" → 1234.56
    assert_eq!(extract_number("¥1,234.56"), Some(1234.56));
    // "￥10000" → 10000
    assert!(extract_number("￥10000").is_some());
}

#[test]
fn test_extract_percent() {
    assert_eq!(extract_number("85%"), Some(0.85));
    assert_eq!(extract_number("100%"), Some(1.0));
    assert_eq!(extract_number("0%"), Some(0.0));
    assert_eq!(extract_number("85.5%"), Some(0.855));
}

#[test]
fn test_extract_empty() {
    assert_eq!(extract_number(""), None);
    assert_eq!(extract_number("   "), None);
}

#[test]
fn test_extract_decimal() {
    assert_eq!(extract_number("3.14"), Some(3.14));
    assert_eq!(extract_number("0.5"), Some(0.5));
}

// ============================================================
// AnalysisQuality 评分测试 (4维度，满分8分，摘要不计分)
// ============================================================

#[test]
fn test_quality_full_score() {
    let content = "2024年前6个月，公司A收入领先序时进度。\n\
                   营业收入：累计 1000 万元，年度达成率 60%，领先序时进度 5.0 个百分点，环比上升，波动平稳。\n\
                   营业收入：累计 800 万元（注：EBITDA相关），环比上升。\n\
                   经营活动净现金流：累计 200 万元，年度达成率 55%。\n\
                   经营支出：累计 500 万元，年度达成率 40%，环比收窄，波动平稳，成本管控有效。";

    let q = AnalysisQuality::from_content("公司A", content, None);
    assert!(q.has_summary, "应该检测到摘要");
    assert!(q.has_revenue, "应该检测到营业收入");
    assert!(q.has_ebitda, "应该检测到EBITDA");  // "EBITDA"
    assert!(q.has_cashflow, "应该检测到现金流");
    assert!(q.has_expense, "应该检测到经营支出");
    assert_eq!(q.score, 8, "满分应为8（4维度×2，摘要不计分）");
}

#[test]
fn test_quality_missing_items() {
    // 仅包含摘要和营业收入
    let content = "2024年前6个月，公司B收入领先。\n营业收入：累计 500 万元，达成率 50%。";

    let q = AnalysisQuality::from_content("公司B", content, None);
    assert!(q.has_summary);
    assert!(q.has_revenue);
    assert!(!q.has_ebitda);
    assert!(!q.has_cashflow);
    assert!(!q.has_expense);
    assert_eq!(q.score, 2, "仅营收=2分（摘要不计分）");
}

#[test]
fn test_quality_empty_content() {
    let q = AnalysisQuality::from_content("空公司", "", None);
    assert!(!q.has_summary);
    assert_eq!(q.total_lines, 0);
    assert_eq!(q.score, 0);
}

#[test]
fn test_quality_no_summary() {
    // 首行直接包含营业收入关键词 → 不算摘要
    let content = "营业收入：累计 500 万元，达成率 50%。";

    let q = AnalysisQuality::from_content("公司C", content, None);
    assert!(!q.has_summary, "首行含关键词不应算摘要");
}

// ============================================================
// Date Utils 测试
// ============================================================

use excelminer_lib::utils::date_utils::{parse_month, ytd_months, parse_date_from_folder};

#[test]
fn test_parse_month_formats() {
    assert_eq!(parse_month("2024年6月"), Some((2024, 6)));
    assert_eq!(parse_month("2024年12月"), Some((2024, 12)));
    assert_eq!(parse_month("2024-06"), Some((2024, 6)));
    assert_eq!(parse_month("2024-1"), Some((2024, 1)));
    assert_eq!(parse_month("abc"), None);
    assert_eq!(parse_month("2024年13月"), None);
}

#[test]
fn test_ytd_months_basic() {
    let months = ytd_months(2024, 6, 5);
    assert_eq!(months, vec![(2024, 2), (2024, 3), (2024, 4), (2024, 5), (2024, 6)]);
}

#[test]
fn test_ytd_months_cross_year() {
    let months = ytd_months(2024, 2, 3);
    assert_eq!(months, vec![(2023, 12), (2024, 1), (2024, 2)]);
}

#[test]
fn test_ytd_months_single() {
    assert_eq!(ytd_months(2024, 1, 1), vec![(2024, 1)]);
}

#[test]
fn test_parse_date_from_folder() {
    assert_eq!(parse_date_from_folder("2024年6月"), Some((2024, 6)));
    assert_eq!(parse_date_from_folder("2024-06"), Some((2024, 6)));
    assert_eq!(parse_date_from_folder("random"), None);
}

// ============================================================
// AI 分析器 + 质量检查器集成测试
// ============================================================

use excelminer_lib::services::ai_analyzer::AIAnalyzer;
use excelminer_lib::services::quality_checker::QualityChecker;
use excelminer_lib::models::project::AIConfig;
use std::path::PathBuf;

#[test]
fn test_ai_config_defaults() {
    let config = AIConfig::default();
    assert_eq!(config.model, "deepseek-v4-pro");
    assert_eq!(config.temperature, 0.3);
    assert_eq!(config.max_tokens, 1500);
    assert_eq!(config.max_retries, 2);
    assert_eq!(config.quality_threshold, 8);
    assert_eq!(config.batch_size, 3);
}

#[test]
fn test_ai_analyzer_creation() {
    let config = AIConfig::default();
    let analyzer = AIAnalyzer::new(config).unwrap();
    assert!(analyzer.load_system_prompt(None).is_ok());
}

#[test]
fn test_ai_default_prompt() {
    let mut config = AIConfig::default();
    config.system_prompt_path = PathBuf::new(); // empty → use default
    let analyzer = AIAnalyzer::new(config).unwrap();
    let prompt = analyzer.load_system_prompt(None).unwrap();
    assert!(!prompt.is_empty());
    assert!(prompt.contains("财务分析"));
}

#[test]
fn test_ai_prompt_from_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let prompt_path = tmp.path().join("test_prompt.md");
    std::fs::write(&prompt_path, "自定义提示词内容").unwrap();

    let mut config = AIConfig::default();
    config.system_prompt_path = prompt_path;
    let analyzer = AIAnalyzer::new(config).unwrap();
    let prompt = analyzer.load_system_prompt(None).unwrap();
    assert_eq!(prompt, "自定义提示词内容");
}

#[test]
fn test_quality_checker_threshold() {
    let checker = QualityChecker::new(8);
    assert_eq!(checker.max_retries(), 2);

    // 满分内容
    let content = "2024年收入领先。\n营业收入：1000万。\nEBITDA：200万。\n经营活动净现金流：300万。\n经营支出：500万。";
    let result = checker.evaluate("测试公司", content, None);
    assert_eq!(result.score, 8);
    assert!(result.passed);
    assert!(result.reason.is_none());
}

#[test]
fn test_quality_checker_low_score() {
    let checker = QualityChecker::new(8);
    let content = "只有营业收入：500万。";
    let result = checker.evaluate("测试公司", content, None);
    assert_eq!(result.score, 2); // 仅营收(2分)，首行含关键词故无摘要分
    assert!(!result.passed);
    assert!(result.reason.is_some());
}

#[test]
fn test_quality_checker_empty() {
    let checker = QualityChecker::new(8);
    assert!(!checker.is_valid_content(""));
    assert!(!checker.is_valid_content("短"));
    // 50字符以上才算有效
    let long_text = "这是一个足够长的有效分析内容文本，确保超过五十个字符的限制，用于验证内容验证功能正常工作";
    assert!(checker.is_valid_content(long_text));
}

#[test]
fn test_quality_hint_generation() {
    let q = AnalysisQuality::from_content("公司X", "仅营收：500万。", None);
    let checker = QualityChecker::new(8);
    let hint = checker.quality_hint(&q);
    assert!(hint.contains("质量提示"));
    assert!(hint.contains(&q.score.to_string()));
}

#[test]
fn test_analyze_batch_structure() {
    // 验证 analyzer 可以创建，不需要真实 API
    let config = AIConfig::default();
    assert!(AIAnalyzer::new(config).is_ok());
}


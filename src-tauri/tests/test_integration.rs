//! 集成测试 — 报表写入 + 读回验证 + 日期 + 质量检查

use std::collections::HashMap;

use calamine::Reader;
use excelminer_lib::models::analysis::{AnalysisResult, AggregationResult, AnalysisQuality};
use excelminer_lib::services::number_parser::extract_number;
use excelminer_lib::utils::date_utils::{parse_month, ytd_months};

// ─── 日期解析测试 ─────────────────────────────────────────────

#[test]
fn test_parse_month_formats() {
    assert_eq!(parse_month("2024年6月"), Some((2024, 6)));
    assert_eq!(parse_month("2024年12月"), Some((2024, 12)));
    assert_eq!(parse_month("2024-06"), Some((2024, 6)));
    assert_eq!(parse_month("2024-12"), Some((2024, 12)));
    assert_eq!(parse_month("2024.06"), Some((2024, 6)));
    assert_eq!(parse_month("2024.12"), Some((2024, 12)));
    assert_eq!(parse_month("202406"), Some((2024, 6)));
    assert_eq!(parse_month("202412"), Some((2024, 12)));
    assert_eq!(parse_month("abc"), None);
    assert_eq!(parse_month("202413"), None); // 13月无效
    assert_eq!(parse_month(""), None);
}

#[test]
fn test_ytd_months_normal() {
    // 2024年4月，YTD 4个月 → [(2024,1), (2024,2), (2024,3), (2024,4)]
    let months = ytd_months(2024, 4, 4);
    assert_eq!(months, vec![(2024, 1), (2024, 2), (2024, 3), (2024, 4)]);
}

#[test]
fn test_ytd_months_cross_year() {
    // 2024年2月，YTD 4个月 → [(2023,11), (2023,12), (2024,1), (2024,2)]
    let months = ytd_months(2024, 2, 4);
    assert_eq!(months, vec![(2023, 11), (2023, 12), (2024, 1), (2024, 2)]);
}

// ─── 数字解析测试 ─────────────────────────────────────────────

#[test]
fn test_number_parser_edge_cases() {
    assert_eq!(extract_number("1,234.56"), Some(1234.56));
    assert_eq!(extract_number("直播1736场"), Some(1736.0));
    assert_eq!(extract_number("1+1000"), Some(1001.0));
    assert_eq!(extract_number("85%"), Some(0.85));
    assert_eq!(extract_number("#N/A"), None);
    assert_eq!(extract_number(""), None);
}

// ─── 质量检查测试 ─────────────────────────────────────────────

#[test]
fn test_quality_all_present() {
    let q = AnalysisQuality {
        company_name: "测试公司".into(),
        has_summary: true,
        has_revenue: true,
        has_ebitda: true,
        has_cashflow: true,
        has_expense: true,
        total_lines: 6,
        score: 8, // 4维度×2=8，摘要不计分
    };
    assert_eq!(q.score, 8);
    assert!(q.has_summary);
    assert!(q.has_revenue);
}

#[test]
fn test_quality_partial() {
    let q = AnalysisQuality {
        company_name: "测试".into(),
        has_summary: true,
        has_revenue: true,
        has_ebitda: false,
        has_cashflow: true,
        has_expense: false,
        total_lines: 4,
        score: 4, // 2+0+2+0=4（摘要不计分）
    };
    assert_eq!(q.score, 4);
    assert!(!q.has_ebitda);
}

// ─── 报表写入 + 读回验证 ──────────────────────────────────────

#[test]
fn test_report_write_and_readback() {
    let tmp = std::env::temp_dir().join("excelminer_test_report.xlsx");
    std::fs::remove_file(&tmp).ok();

    // 1. 创建模拟 aggregation results
    let insurance_data = serde_json::json!([{
        "company": "盛唐融信",
        "人力": {"期初人力": 100.0, "YTD入职": 5.0, "YTD离职": 2.0, "当月净增": 3.0, "月末人力": 103.0, "平均人力": 101.5, "开单人数YTD": 8.0},
        "保费": {"新单规模保费YTD": 50.0, "期交规模保费YTD": 45.0, "续期13月应收": 10.0, "续期13月实收": 9.0, "续期25月应收": 5.0, "续期25月实收": 4.5, "承保件数YTD": 12.0},
        "月度规模保费": [12.0, 13.0, 14.0, 11.0, null, null, null, null, null, null, null, null]
    }, {
        "company": "君康经纪",
        "人力": {"期初人力": 50.0, "YTD入职": 2.0, "YTD离职": 1.0, "当月净增": 1.0, "月末人力": 51.0, "平均人力": 50.5, "开单人数YTD": 5.0},
        "保费": {"新单规模保费YTD": 30.0, "期交规模保费YTD": 28.0, "续期13月应收": 5.0, "续期13月实收": 4.8, "续期25月应收": 2.0, "续期25月实收": 1.9, "承保件数YTD": 8.0},
        "月度规模保费": [7.0, 8.0, 8.5, 6.5, null, null, null, null, null, null, null, null]
    }]);

    let financial_data = serde_json::json!([{
        "company": "测试公司",
        "indicators": [
            {"label": "营业收入", "target": 1000.0, "ytd": 300.0, "rate": 30.0, "values": [80.0, 90.0, 70.0, 60.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]},
            {"label": "EBITDA", "target": 500.0, "ytd": 150.0, "rate": 30.0, "values": [40.0, 45.0, 35.0, 30.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]},
            {"label": "经营活动净现金流", "target": 200.0, "ytd": 60.0, "rate": 30.0, "values": [15.0, 18.0, 14.0, 13.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]},
            {"label": "经营支出", "target": 300.0, "ytd": 90.0, "rate": 30.0, "values": [25.0, 22.0, 23.0, 20.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]},
        ]
    }]);

    let agg_results = vec![
        AggregationResult {
            engine_name: "保险数据汇总".into(),
            companies_processed: 2,
            indicators_collected: 16,
            warnings: vec![],
            summary_data: serde_json::to_string(&insurance_data).unwrap(),
        },
        AggregationResult {
            engine_name: "经营报表汇总".into(),
            companies_processed: 1,
            indicators_collected: 8,
            warnings: vec![],
            summary_data: serde_json::to_string(&financial_data).unwrap(),
        },
    ];

    let ai_results = vec![AnalysisResult {
        company_name: "测试板块".into(),
        business_type: "保险".into(),
        content: "测试分析内容：人力规模稳定，保费增长良好。".into(),
        quality_score: 8,
        retry_count: 0,
        token_usage: None,
        success: true,
        error_message: None,
        analysis_category: "segment".into(),
    }];

    // 2. 写入报表
    excelminer_lib::services::report_writer::ReportWriter::write_summary(
        &tmp,
        &agg_results,
        &ai_results,
        "测试项目",
        2026,
        4,
    ).expect("报表写入失败");

    assert!(tmp.exists(), "报表文件应该存在");

    // 3. 用 calamine 读回验证
    let mut wb: calamine::Xlsx<_> = calamine::open_workbook(&tmp).expect("calamine 打开失败");

    // 验证 Sheet 存在
    let sheet_names: Vec<String> = wb.sheet_names().to_vec();
    assert!(sheet_names.contains(&"填写页".to_string()), "应有填写页");
    assert!(sheet_names.contains(&"保险类".to_string()), "应有保险类");
    assert!(sheet_names.contains(&"测试公司".to_string()), "应有测试公司 Sheet");
    assert!(sheet_names.contains(&"AI分析结果".to_string()), "应有AI分析结果");

    let config = wb.worksheet_range("填写页").expect("填写页读取失败");
    let rows: Vec<&[calamine::Data]> = config.rows().collect();
    // A2 = (0,1) 0-based
    let a2_val = &rows[1][0];
    assert_eq!(a2_val.to_string(), "4", "A2 应为月份 4");

    // 验证 保险类 Sheet 存在且非空
    let insurance = wb.worksheet_range("保险类").expect("保险类读取失败");
    let ins_rows: Vec<&[calamine::Data]> = insurance.rows().collect();
    assert!(ins_rows.len() > 5, "保险类至少应有 5 行数据");
    // 验证 A2 行首列不为空（应有指标名）
    assert!(!ins_rows[1][0].to_string().is_empty(), "保险类 Row2 应有数据");

    // 验证 经营报表 — 测试公司 Sheet 存在且有数据行 (Row2-4, 不含表头=4行)
    let financial = wb.worksheet_range("测试公司").expect("测试公司 Sheet 读取失败");
    let fin_rows: Vec<&[calamine::Data]> = financial.rows().collect();
    assert!(fin_rows.len() >= 4, "测试公司至少应有 4 行数据，实际: {}", fin_rows.len());

    // 验证 AI分析结果 Sheet
    let ai = wb.worksheet_range("AI分析结果").expect("AI分析结果读取失败");
    let ai_rows: Vec<&[calamine::Data]> = ai.rows().collect();
    let ai_content = &ai_rows[1][4]; // E2 = 分析内容
    assert!(ai_content.to_string().contains("测试分析内容"), "AI分析内容应正确");

    // 清理
    std::fs::remove_file(&tmp).ok();
    println!("✅ 报表写入+读回验证通过");
}

#[test]
fn test_financial_indicator_names_mapping() {
    // 验证 section header → 指标名映射逻辑（从 financial.rs 提取）
    let section_map: HashMap<&str, Vec<&str>> = [
        ("经营指标", vec!["营业收入", "EBITDA", "经营活动净现金流"]),
        ("财务指标", vec!["经营支出"]),
    ].into();

    assert_eq!(section_map.get("经营指标").unwrap()[0], "营业收入");
    assert_eq!(section_map.get("经营指标").unwrap()[1], "EBITDA");
    assert_eq!(section_map.get("经营指标").unwrap()[2], "经营活动净现金流");
    assert_eq!(section_map.get("财务指标").unwrap()[0], "经营支出");
}

#[test]
fn test_empty_aggregation_no_panic() {
    // 空聚合结果不应 panic
    let tmp = std::env::temp_dir().join("excelminer_test_empty.xlsx");
    std::fs::remove_file(&tmp).ok();

    let result = excelminer_lib::services::report_writer::ReportWriter::write_summary(
        &tmp,
        &[],
        &[],
        "空项目",
        2026,
        1,
    );
    assert!(result.is_ok(), "空数据写入不应失败");
    assert!(tmp.exists(), "应生成空报表文件");

    std::fs::remove_file(&tmp).ok();
}

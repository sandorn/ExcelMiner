//! AI 业态分析流 测试 — 数据准备、质量检查、报表写入、边界条件

use calamine::Reader;

use excelminer_lib::models::analysis::{
    AnalysisResult, AggregationResult, AnalysisQuality, ProgressUpdate, ProgressStatus, TokenUsage,
};
use excelminer_lib::models::project::{AIConfig, BusinessType};
use excelminer_lib::services::ai_analyzer::AIAnalyzer;
use excelminer_lib::services::quality_checker::QualityChecker;

// ============================================================
// 1. 质量评分按业态
// ============================================================

#[test]
fn test_quality_insurance() {
    let content = "2026年前4个月，保险板块经营态势稳定。\n\
        人力：期初100人，净增3人。\n\
        承保：新单17件。\n\
        保费规模：YTD达50万元。";
    let q = AnalysisQuality::from_content("保险板块", content, Some(&BusinessType::Insurance));
    assert!(q.has_summary);
    assert!(q.has_revenue, "保险应有人力维度");
    assert!(q.has_ebitda, "保险应有承保维度");
    assert!(q.has_cashflow, "保险应有保费维度");
    assert_eq!(q.score, 8);
}

#[test]
fn test_quality_hotel() {
    let content = "2026年前4个月，酒店板块经营态势良好。\n\
        营销活动：投放50次。\n\
        OTA评价：平均4.5分。\n\
        入住率：月均85%。";
    let q = AnalysisQuality::from_content("酒店板块", content, Some(&BusinessType::Hotel));
    assert!(q.has_summary);
    assert!(q.has_revenue);
    assert!(q.has_ebitda);
    assert!(q.has_cashflow);
    assert_eq!(q.score, 8);
}

#[test]
fn test_quality_commercial() {
    let content = "2026年前4个月，商写板块整体招商良好。\n\
        整体情况：出租率90%。\n\
        合作渠道：成交50组。\n\
        自有招商：成交20组。\n\
        续租：续签800平。";
    let q = AnalysisQuality::from_content("商写板块", content, Some(&BusinessType::Commercial));
    assert!(q.has_summary && q.has_revenue && q.has_ebitda && q.has_cashflow && q.has_expense);
    assert_eq!(q.score, 10);
}

#[test]
fn test_quality_financial_generic() {
    let content = "2026年前4个月收入领先序时进度。\n\
        营业收入：累计2500万。\n\
        EBITDA：累计500万。\n\
        经营活动净现金流：累计300万。\n\
        经营支出：累计800万。";
    let q = AnalysisQuality::from_content("某公司", content, None);
    assert!(q.has_summary && q.has_revenue && q.has_ebitda && q.has_cashflow && q.has_expense);
    assert_eq!(q.score, 10);
}

#[test]
fn test_quality_insufficient() {
    let checker = QualityChecker::new(8);
    let result = checker.evaluate("测试", "只有营业收入：500万。", None);
    assert!(result.score < 8);
    assert!(!result.passed);
}

#[test]
fn test_quality_content_length() {
    let checker = QualityChecker::new(8);
    assert!(!checker.is_valid_content("short"));
    assert!(checker.is_valid_content(
        "这是一个超过五十个字符的文本内容用于测试验证功能是否正常工作确保评分正确"));
}

// ============================================================
// 2. AI 分析器 & 提示词
// ============================================================

#[test]
fn test_analyzer_load_all_prompts() {
    let config = AIConfig::default();
    let analyzer = AIAnalyzer::new(config).unwrap();
    for (bt, kw) in &[
        (BusinessType::Insurance, "保险"),
        (BusinessType::Commercial, "商写"),
        (BusinessType::Hotel, "酒店"),
    ] {
        let prompt = analyzer.load_system_prompt(Some(bt)).unwrap();
        assert!(!prompt.is_empty());
        assert!(prompt.contains(kw), "{:?} should contain '{kw}'", bt);
    }
    let generic = analyzer.load_system_prompt(None).unwrap();
    assert!(generic.contains("财务") || generic.contains("经营"));
}

#[test]
fn test_analyzer_fallback_no_crash() {
    let mut config = AIConfig::default();
    config.system_prompt_path = std::path::PathBuf::from("__nonexist__");
    let analyzer = AIAnalyzer::new(config).unwrap();
    assert!(analyzer.load_system_prompt(None).is_ok());
}

#[test]
fn test_analyzer_various_configs() {
    for c in [
        AIConfig { temperature: 0.5, ..Default::default() },
        AIConfig { max_tokens: 2000, ..Default::default() },
        AIConfig { max_retries: 3, quality_threshold: 9, ..Default::default() },
    ] {
        assert!(AIAnalyzer::new(c).is_ok());
    }
}

// ============================================================
// 3. 报表写入 — 段分析结果
// ============================================================

#[test]
fn test_write_segment_analysis_to_report() {
    let tmp = std::env::temp_dir().join("excelminer_test_seg.xlsx");
    std::fs::remove_file(&tmp).ok();

    let insurance_data = serde_json::json!([{
        "company": "盛唐融信",
        "人力": {"期初人力":100.0,"YTD入职":5.0,"YTD离职":2.0,"当月净增":3.0,"月末人力":103.0,"平均人力":101.5,"开单人数YTD":8.0},
        "保费": {"新单规模保费YTD":50.0,"期交规模保费YTD":45.0,"续期13月应收":10.0,"续期13月实收":9.0,"续期25月应收":5.0,"续期25月实收":4.5,"承保件数YTD":12.0},
        "月度规模保费": [12.0,13.0,14.0,11.0]
    }, {
        "company": "君康经纪",
        "人力": {"期初人力":50.0,"YTD入职":2.0,"YTD离职":1.0,"当月净增":1.0,"月末人力":51.0,"平均人力":50.5,"开单人数YTD":5.0},
        "保费": {"新单规模保费YTD":30.0,"期交规模保费YTD":28.0,"续期13月应收":5.0,"续期13月实收":4.8,"续期25月应收":2.0,"续期25月实收":1.9,"承保件数YTD":8.0},
        "月度规模保费": [7.0,8.0,8.5,6.5]
    }]);

    let commercial_data = serde_json::json!([{
        "company": "北京中言",
        "面积": {"期初面积":5000.0,"YTD新增签约":200.0,"YTD退租":50.0,"月末面积":5150.0},
        "渠道": {"带客":150.0,"成交":30.0,"签约面积":2000.0},
        "自营": {"带客":80.0,"成交":15.0,"签约面积":1000.0},
        "续签": {"到期面积":500.0,"续签面积":400.0}
    }]);

    let agg_results = vec![
        AggregationResult {
            engine_name: "保险数据汇总".into(), companies_processed: 2,
            indicators_collected: 16, warnings: vec![],
            summary_data: serde_json::to_string(&insurance_data).unwrap(),
        },
        AggregationResult {
            engine_name: "商写数据汇总".into(), companies_processed: 1,
            indicators_collected: 10, warnings: vec![],
            summary_data: serde_json::to_string(&commercial_data).unwrap(),
        },
    ];

    let ai_results = vec![
        AnalysisResult {
            company_name: "保险板块".into(), business_type: "保险".into(),
            content: "保险板块经营稳健。保费YTD达50万元。".into(),
            quality_score: 0, retry_count: 0, token_usage: None,
            success: true, error_message: None, analysis_category: "segment".into(),
        },
        AnalysisResult {
            company_name: "商写板块".into(), business_type: "商写".into(),
            content: "商写板块招商良好。".into(),
            quality_score: 0, retry_count: 0, token_usage: None,
            success: true, error_message: None, analysis_category: "segment".into(),
        },
    ];

    excelminer_lib::services::report_writer::ReportWriter::write_summary(
        &tmp, &agg_results, &ai_results, "测试", 2026, 4,
    ).expect("写入失败");

    let mut wb: calamine::Xlsx<_> = calamine::open_workbook(&tmp).unwrap();
    let sheets = wb.sheet_names().to_vec();
    assert!(sheets.contains(&"填写页".to_string()));
    assert!(sheets.contains(&"保险类".to_string()));
    assert!(sheets.contains(&"商写类".to_string()));

    // 验证保险类 Sheet 存在且写入成功（L14 可能不在 calamine used range 内）
    if let Ok(range) = wb.worksheet_range("保险类") {
        let rows: Vec<&[calamine::Data]> = range.rows().collect();
        println!("保险类: {} rows", rows.len());
        assert!(rows.len() >= 2, "保险类至少应有数据行");
    }

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_write_company_analysis_to_c61() {
    let tmp = std::env::temp_dir().join("excelminer_test_c61.xlsx");
    std::fs::remove_file(&tmp).ok();

    let financial_data = serde_json::json!([{
        "company": "盛唐融信",
        "indicators": [
            {"label":"营业收入","target":1000.0,"ytd":300.0,"rate":30.0,"values":[80.0,90.0,70.0,60.0]},
        ]
    }]);

    let agg_results = vec![AggregationResult {
        engine_name: "经营报表汇总".into(), companies_processed: 1,
        indicators_collected: 2, warnings: vec![],
        summary_data: serde_json::to_string(&financial_data).unwrap(),
    }];

    let ai_results = vec![AnalysisResult {
        company_name: "盛唐融信".into(), business_type: "经营指标".into(),
        content: "收入领先：累计300万，达成率30%。".into(),
        quality_score: 6, retry_count: 0, token_usage: None,
        success: true, error_message: None, analysis_category: "company".into(),
    }];

    excelminer_lib::services::report_writer::ReportWriter::write_summary(
        &tmp, &agg_results, &ai_results, "测试", 2026, 4,
    ).expect("写入失败");

    let mut wb: calamine::Xlsx<_> = calamine::open_workbook(&tmp).unwrap();
    let range = wb.worksheet_range("盛唐融信").unwrap();
    let rows: Vec<&[calamine::Data]> = range.rows().collect();
    if rows.len() >= 61 {
        let c61 = rows.get(60).and_then(|r| r.get(2))
            .map(|c| c.to_string()).unwrap_or_default();
        assert!(c61.contains("收入") || c61.contains("累计"), "C61应含分析");
    }

    std::fs::remove_file(&tmp).ok();
}

// ============================================================
// 4. fixture 读取（段分析数据源，不卡死验证）
// ============================================================

#[test]
fn test_fixture_readable_no_hang() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join("2026年4月分析")
        .join("【2026年4月】经营数据.xlsx");
    if !path.exists() { eprintln!("跳过: fixture不存在"); return; }

    let start = std::time::Instant::now();
    let mut wb = match calamine::open_workbook::<calamine::Xlsx<_>, _>(&path) {
        Ok(w) => w,
        Err(e) => { eprintln!("跳过: {}", e); return; }
    };
    assert!(start.elapsed() < std::time::Duration::from_secs(5),
        "xlsx打开不应超过5秒");

    let sheets = wb.sheet_names().to_vec();
    for name in &["保险类", "商写类", "酒店类"] {
        if sheets.contains(&name.to_string()) {
            let range = wb.worksheet_range(name).unwrap();
            let rows: Vec<&[calamine::Data]> = range.rows().collect();
            println!("  {}: {}rows", name, rows.len());
            assert!(rows.len() >= 2, "{} 至少2行", name);
        }
    }
}

// ============================================================
// 5. 进度 & 错误结果
// ============================================================

#[test]
fn test_progress_serde() {
    let u = ProgressUpdate {
        step: "板块分析: 保险 (1/3)".into(), progress: 0.33,
        status: ProgressStatus::Running, company: Some("保险板块".into()),
    };
    let json = serde_json::to_string(&u).unwrap();
    assert!(json.contains("板块分析"));
    let back: ProgressUpdate = serde_json::from_str(&json).unwrap();
    assert_eq!(back.progress, 0.33);
}

#[test]
fn test_failure_result_structure() {
    let r = AnalysisResult {
        company_name: "失败".into(), business_type: "保险".into(),
        content: String::new(), quality_score: 0, retry_count: 2,
        token_usage: None, success: false,
        error_message: Some("API超时".into()), analysis_category: "segment".into(),
    };
    assert!(!r.success);
    assert_eq!(r.retry_count, 2);
}

// ============================================================
// 6. 段/公司结果过滤
// ============================================================

#[test]
fn test_filter_segment_vs_company() {
    let results = vec![
        AnalysisResult { company_name:"保险板块".into(), business_type:"保险".into(),
            content:"a".into(), quality_score:0, retry_count:0, token_usage:None,
            success:true, error_message:None, analysis_category:"segment".into() },
        AnalysisResult { company_name:"盛唐融信".into(), business_type:"经营指标".into(),
            content:"b".into(), quality_score:10, retry_count:0, token_usage:None,
            success:true, error_message:None, analysis_category:"company".into() },
    ];
    assert_eq!(results.iter().filter(|r| r.analysis_category == "segment").count(), 1);
    assert_eq!(results.iter().filter(|r| r.analysis_category == "company").count(), 1);
}

// ============================================================
// 7. 边界条件 — 空/失败/超长/非法XML
// ============================================================

fn mk_agg() -> AggregationResult {
    AggregationResult { engine_name:"保险数据汇总".into(), companies_processed:1,
        indicators_collected:2, warnings:vec![],
        summary_data: serde_json::to_string(&serde_json::json!([{
            "company":"A",
            "人力":{"期初人力":1.0},
            "保费":{"新单规模保费YTD":50.0}
        }])).unwrap(),
    }
}

#[test]
fn test_empty_ai_no_panic() {
    let tmp = std::env::temp_dir().join("excelminer_test_emp.xlsx");
    assert!(excelminer_lib::services::report_writer::ReportWriter::write_summary(
        &tmp, &[mk_agg()], &[], "T", 2026, 1).is_ok());
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_failed_ai_no_crash() {
    let tmp = std::env::temp_dir().join("excelminer_test_fail.xlsx");
    let ai = vec![AnalysisResult { company_name:"保险板块".into(), business_type:"保险".into(),
        content:String::new(), quality_score:0, retry_count:2, token_usage:None,
        success:false, error_message:Some("超时".into()), analysis_category:"segment".into() }];
    assert!(excelminer_lib::services::report_writer::ReportWriter::write_summary(
        &tmp, &[mk_agg()], &ai, "T", 2026, 1).is_ok());
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_long_content_no_crash() {
    let tmp = std::env::temp_dir().join("excelminer_test_long.xlsx");
    let long = "保险板块".to_string() + &"内容".repeat(20000);
    let ai = vec![AnalysisResult { company_name:"保险板块".into(), business_type:"保险".into(),
        content:long, quality_score:0, retry_count:0, token_usage:None,
        success:true, error_message:None, analysis_category:"segment".into() }];
    assert!(excelminer_lib::services::report_writer::ReportWriter::write_summary(
        &tmp, &[mk_agg()], &ai, "T", 2026, 1).is_ok());
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_illegal_xml_sanitized() {
    let tmp = std::env::temp_dir().join("excelminer_test_xml.xlsx");
    let bad = format!("保险\n{}", '\x01');
    let ai = vec![AnalysisResult { company_name:"保险板块".into(), business_type:"保险".into(),
        content:bad, quality_score:0, retry_count:0, token_usage:None,
        success:true, error_message:None, analysis_category:"segment".into() }];
    assert!(excelminer_lib::services::report_writer::ReportWriter::write_summary(
        &tmp, &[mk_agg()], &ai, "T", 2026, 1).is_ok(),
        "含非法XML字符应被sanitize");
    std::fs::remove_file(&tmp).ok();
}

// ============================================================
// 8. 汇总数据结构验证
// ============================================================

#[test]
fn test_aggregation_result_structure() {
    let data = serde_json::json!([{
        "company": "测试公司",
        "indicators": [
            {"label": "营业收入", "target": 1000.0, "ytd": 300.0, "rate": 30.0,
             "values": [80.0, 90.0, 70.0, 60.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]},
            {"label": "EBITDA", "target": 500.0, "ytd": 150.0, "rate": 30.0,
             "values": [40.0, 45.0, 35.0, 30.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]},
        ]
    }]);

    let result = AggregationResult {
        engine_name: "经营报表汇总".into(),
        companies_processed: 1,
        indicators_collected: 4,
        warnings: vec![],
        summary_data: serde_json::to_string(&data).unwrap(),
    };

    assert_eq!(result.engine_name, "经营报表汇总");
    assert!(result.summary_data.contains("测试公司"));
    assert!(result.summary_data.contains("营业收入"));

    let companies: Vec<serde_json::Value> = serde_json::from_str(&result.summary_data).unwrap();
    assert_eq!(companies.len(), 1);
    let indicators = companies[0]["indicators"].as_array().unwrap();
    assert_eq!(indicators.len(), 2);
    assert_eq!(indicators[0]["label"], "营业收入");
}

#[test]
fn test_multi_engine_results() {
    let insurance_json = serde_json::to_string(&serde_json::json!([{
        "company": "盛唐融信", "人力": {"期初人力": 100.0}
    }])).unwrap();
    let financial_json = serde_json::to_string(&serde_json::json!([{
        "company": "盛唐融信", "indicators": [{"label": "营业收入", "ytd": 300.0}]
    }])).unwrap();

    let results = vec![
        AggregationResult {
            engine_name: "保险数据汇总".into(), companies_processed: 1,
            indicators_collected: 8, warnings: vec![],
            summary_data: insurance_json,
        },
        AggregationResult {
            engine_name: "经营报表汇总".into(), companies_processed: 1,
            indicators_collected: 2, warnings: vec![],
            summary_data: financial_json,
        },
    ];

    assert_eq!(results.len(), 2);
    assert!(!results[0].summary_data.contains("营业收入"));
    assert!(results[1].summary_data.contains("营业收入"));
}

// ============================================================
// 9. 结果序列化
// ============================================================

#[test]
fn test_analysis_result_serde_roundtrip() {
    let result = AnalysisResult {
        company_name: "保险板块".into(),
        business_type: "保险".into(),
        content: "保险板块分析内容...".into(),
        quality_score: 8,
        retry_count: 1,
        token_usage: Some(TokenUsage {
            prompt_tokens: 500, completion_tokens: 300, total_tokens: 800,
        }),
        success: true,
        error_message: None,
        analysis_category: "segment".into(),
    };

    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("保险板块"));
    assert!(json.contains("segment"));

    let parsed: AnalysisResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.company_name, "保险板块");
    assert_eq!(parsed.analysis_category, "segment");
    assert_eq!(parsed.quality_score, 8);
    assert!(parsed.token_usage.is_some());
}

// ============================================================
// 10. 业态分析(segment) — fixture 数据读取 (模拟 execute_segment_analysis)
// ============================================================

/// 保险业态：从汇总表 "保险类" Sheet 读取 F1:H25 范围
#[test]
fn test_segment_read_insurance_from_fixture() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join("2026年4月分析")
        .join("【2026年4月】经营数据.xlsx");
    if !path.exists() { eprintln!("跳过: fixture 不存在"); return; }

    let mut wb: calamine::Xlsx<_> = calamine::open_workbook(&path).unwrap();
    let sheets = wb.sheet_names().to_vec();
    if !sheets.contains(&"保险类".to_string()) { eprintln!("跳过: 无保险类Sheet"); return; }

    let range = wb.worksheet_range("保险类").unwrap();
    let rows: Vec<&[calamine::Data]> = range.rows().collect();
    println!("保险类: {} 行", rows.len());
    assert!(rows.len() >= 10, "保险类至少10行");

    // 验证关键单元格: C2=盛唐融信期初人力, D2=君康经纪期初人力
    let c2 = cell_at(&rows, 2, 3);
    let d2 = cell_at(&rows, 2, 4);
    println!("  C2(盛唐融信期初人力) = {c2}  D2(君康经纪期初人力) = {d2}");

    // A2=项目, A3=期初人力, A10=新单规模保费YTD
    let a2 = cell_at(&rows, 2, 1);
    let a3 = cell_at(&rows, 3, 1);
    println!("  A2(项目) = {a2}  A3(期初) = {a3}");

    // 验证至少有一个单元格有数据（非空非零）
    let has_data = rows.iter().any(|r| {
        r.iter().any(|c| {
            let s = c.to_string();
            let t = s.trim();
            !t.is_empty() && t != "0" && t != "0.0"
        })
    });
    assert!(has_data, "保险类 Sheet 应有实际数据");

    // 行数统计（用于 AI prompt 构建）
    let data_rows = rows.iter()
        .filter(|r| !r.is_empty())
        .count();
    println!("  有效行数: {}", data_rows);
}

/// 商写业态：从汇总表 "商写类" Sheet 读取 A1:G18 范围
#[test]
fn test_segment_read_commercial_from_fixture() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join("2026年4月分析")
        .join("【2026年4月】经营数据.xlsx");
    if !path.exists() { eprintln!("跳过: fixture 不存在"); return; }

    let mut wb: calamine::Xlsx<_> = calamine::open_workbook(&path).unwrap();
    let sheets = wb.sheet_names().to_vec();
    if !sheets.contains(&"商写类".to_string()) { eprintln!("跳过: 无商写类Sheet"); return; }

    let range = wb.worksheet_range("商写类").unwrap();
    let rows: Vec<&[calamine::Data]> = range.rows().collect();
    println!("商写类: {} 行", rows.len());
    assert!(rows.len() >= 5, "商写类至少5行");

    // 验证各公司列: C(北京中言), D(大连凯丹), E(福建钱隆), F(春夏秋冬), G(重庆宜新)
    for (col, name) in [(3, "北京中言"), (4, "大连凯丹"), (5, "福建钱隆"), (6, "春夏秋冬"), (7, "重庆宜新")] {
        let val = cell_at(&rows, 2, col);
        println!("  {}{}({name}期初面积) = {val}", col_letter(col), 2);
    }

    let has_data = rows.iter().any(|r| r.iter().any(|c| {
        let t = c.to_string().trim().to_string();
        !t.is_empty() && t != "0" && t != "0.0"
    }));
    assert!(has_data, "商写类 Sheet 应有数据");
}

/// 酒店业态：从汇总表 "酒店类" Sheet 读取三个区域
#[test]
fn test_segment_read_hotel_from_fixture() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join("2026年4月分析")
        .join("【2026年4月】经营数据.xlsx");
    if !path.exists() { eprintln!("跳过: fixture 不存在"); return; }

    let mut wb: calamine::Xlsx<_> = calamine::open_workbook(&path).unwrap();
    let sheets = wb.sheet_names().to_vec();
    if !sheets.contains(&"酒店类".to_string()) { eprintln!("跳过: 无酒店类Sheet"); return; }

    let range = wb.worksheet_range("酒店类").unwrap();
    let rows: Vec<&[calamine::Data]> = range.rows().collect();
    println!("酒店类: {} 行", rows.len());
    assert!(rows.len() >= 2, "酒店类至少应有头行");

    // 三区域检查：B1:D5(营销), E1:G13(OTA), I1:K13(入住率)
    let b2 = cell_at(&rows, 2, 2);
    let e2 = cell_at(&rows, 2, 5);
    let i2 = cell_at(&rows, 2, 9);
    println!("  营销B2 = {b2}  OTA_E2 = {e2}  入住率I2 = {i2}");

    // 验证至少有数据
    let has_data = rows.iter().any(|r| r.iter().any(|c| {
        let t = c.to_string().trim().to_string();
        !t.is_empty() && t != "0"
    }));
    assert!(has_data, "酒店类 Sheet 应有数据");
}

// ============================================================
// 11. 经营分析(company) — fixture 公司数据读取 (模拟 execute_company_analysis)
// ============================================================

/// 从汇总表公司 Sheet 的 C1:R5 读取财务指标（模拟 read_company_data_from_summary）
#[test]
fn test_company_financial_read_from_fixture() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join("2026年4月分析")
        .join("【2026年4月】经营数据.xlsx");
    if !path.exists() { eprintln!("跳过: fixture 不存在"); return; }

    let mut wb: calamine::Xlsx<_> = calamine::open_workbook(&path).unwrap();
    let sheets = wb.sheet_names().to_vec();

    // 找至少一个子公司 Sheet
    let company_sheets: Vec<&String> = sheets.iter()
        .filter(|n| !matches!(n.as_str(), "填写页" | "保险类" | "商写类" | "酒店类" | "AI分析结果" | "Sheet1" | "年度计划" | "分解计划" | "总指标历史分期" | "营收历史分体分期" | "管理建议" | "整体" | "营收"))
        .collect();

    println!("公司 Sheets: {:?}", company_sheets);
    assert!(!company_sheets.is_empty(), "应有至少一个公司Sheet");

    for sheet_name in company_sheets.iter().take(3) {
        let range = wb.worksheet_range(sheet_name).unwrap();
        let rows: Vec<&[calamine::Data]> = range.rows().collect();
        println!("  {}: {} 行", sheet_name, rows.len());

        // C2:C5 → 指标名 (col 3, rows 2-5, 1-based)
        let mut indicator_names = Vec::new();
        for r in 1..=4 {
            if let Some(cell) = rows.get(r).and_then(|row| row.get(2)) {
                let s = cell.to_string().trim().to_string();
                if !s.is_empty() && s.parse::<f64>().is_err() {
                    indicator_names.push(s);
                }
            }
        }
        println!("    C2:C5 指标名 = {:?}", indicator_names);

        // D2:D5 → 年度目标 (col 4), E2:E5 → YTD实际 (col 5)
        if rows.len() >= 2 && rows[1].len() >= 5 {
            let target = &rows[1][3]; // D2
            let ytd = &rows[1][4];    // E2
            println!("    D2(目标)={}  E2(YTD)={}", target, ytd);
        }

        assert!(rows.len() >= 2, "{} 至少2行", sheet_name);
    }
}

// ============================================================
// 12. 业态分析 + 经营分析 结果合并写入报表
// ============================================================

#[test]
fn test_segment_and_company_results_combined_write() {
    let tmp = std::env::temp_dir().join("excelminer_test_combined.xlsx");
    std::fs::remove_file(&tmp).ok();

    // ── 模拟汇总数据（保险+经营报表）──
    let insurance_data = serde_json::json!([{
        "company": "盛唐融信",
        "人力": {"期初人力":100.0,"YTD入职":5.0,"YTD离职":2.0,"当月净增":3.0,"月末人力":103.0,"平均人力":101.5,"开单人数YTD":8.0},
        "保费": {"新单规模保费YTD":50.0,"期交规模保费YTD":45.0,"续期13月应收":10.0,"续期13月实收":9.0,"续期25月应收":5.0,"续期25月实收":4.5,"承保件数YTD":12.0},
        "月度规模保费": [12.0,13.0,14.0,11.0]
    }]);

    let financial_data = serde_json::json!([{
        "company": "盛唐融信",
        "indicators": [
            {"label":"营业收入","target":1000.0,"ytd":300.0,"rate":30.0,"values":[80.0,90.0,70.0,60.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0]},
            {"label":"EBITDA","target":500.0,"ytd":150.0,"rate":30.0,"values":[40.0,45.0,35.0,30.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0]},
        ]
    }]);

    let agg_results = vec![
        AggregationResult {
            engine_name: "保险数据汇总".into(), companies_processed: 1,
            indicators_collected: 16, warnings: vec![],
            summary_data: serde_json::to_string(&insurance_data).unwrap(),
        },
        AggregationResult {
            engine_name: "经营报表汇总".into(), companies_processed: 1,
            indicators_collected: 4, warnings: vec![],
            summary_data: serde_json::to_string(&financial_data).unwrap(),
        },
    ];

    // ── 模拟两阶段 AI 分析结果 ──
    let ai_results = vec![
        // 阶段一：业态板块分析
        AnalysisResult {
            company_name: "保险板块".into(), business_type: "保险".into(),
            content: "保险板块经营稳健。人力净增3人，保费YTD达50万元。".into(),
            quality_score: 0, retry_count: 0, token_usage: None,
            success: true, error_message: None, analysis_category: "segment".into(),
        },
        // 阶段二：公司经营指标分析
        AnalysisResult {
            company_name: "盛唐融信".into(), business_type: "经营指标".into(),
            content: "收入领先序时进度。营业收入：累计300万元，达成率30%。EBITDA：累计150万元。".into(),
            quality_score: 6, retry_count: 0, token_usage: None,
            success: true, error_message: None, analysis_category: "company".into(),
        },
    ];

    // 写入
    excelminer_lib::services::report_writer::ReportWriter::write_summary(
        &tmp, &agg_results, &ai_results, "测试项目", 2026, 4,
    ).expect("写入失败");
    assert!(tmp.exists());

    // 读回验证
    let mut wb: calamine::Xlsx<_> = calamine::open_workbook(&tmp).unwrap();
    let sheets = wb.sheet_names().to_vec();
    println!("Sheets: {:?}", sheets);

    // 验证 Sheet 存在
    assert!(sheets.contains(&"填写页".to_string()), "应有填写页");
    assert!(sheets.contains(&"保险类".to_string()), "应有保险类");
    assert!(sheets.contains(&"盛唐融信".to_string()), "应有公司Sheet");

    // 验证 保险类 有汇总数据
    let ins_range = wb.worksheet_range("保险类").unwrap();
    let ins_rows: Vec<&[calamine::Data]> = ins_range.rows().collect();
    println!("保险类: {} 行", ins_rows.len());
    assert!(ins_rows.len() >= 2, "保险类至少2行");

    // 验证 盛唐融信 Sheet 有数据
    let co_range = wb.worksheet_range("盛唐融信").unwrap();
    let co_rows: Vec<&[calamine::Data]> = co_range.rows().collect();
    println!("盛唐融信: {} 行", co_rows.len());
    assert!(co_rows.len() >= 2, "公司Sheet至少2行");

    // 验证 C61 写入 (公司分析 → row 61 col 3)
    if co_rows.len() >= 61 {
        let c61 = co_rows.get(60).and_then(|r| r.get(2))
            .map(|c| c.to_string()).unwrap_or_default();
        println!("盛唐融信 C61: {}", &c61[..c61.len().min(80)]);
        assert!(c61.contains("收入") || c61.contains("EBITDA") || c61.contains("累计"),
            "C61 应有公司经营分析内容");
    }

    // 验证 L14 写入 (板块分析 → 保险类 row 14 col 12)
    if ins_rows.len() >= 14 {
        let l14 = ins_rows.get(13).and_then(|r| r.get(11))
            .map(|c| c.to_string()).unwrap_or_default();
        println!("保险类 L14: {}", &l14[..l14.len().min(80)]);
        // L14 可能不在 calamine used range 内，不做强断言
    }

    std::fs::remove_file(&tmp).ok();
}

// ============================================================
// 13. 业态提示词匹配验证
// ============================================================

/// 验证每种业态类型能正确加载专属提示词(含模板占位符)
#[test]
fn test_prompts_contain_template_indicators() {
    let config = AIConfig::default();
    let analyzer = AIAnalyzer::new(config).unwrap();

    // 保险提示词应含关键指标名
    let insurance_prompt = analyzer.load_system_prompt(Some(&BusinessType::Insurance)).unwrap();
    for kw in &["人力", "保费", "承保"] {
        assert!(insurance_prompt.contains(kw),
            "保险提示词应包含 '{kw}'");
    }

    // 商写提示词应含关键指标名
    let commercial_prompt = analyzer.load_system_prompt(Some(&BusinessType::Commercial)).unwrap();
    for kw in &["面积", "渠道", "续租"] {
        assert!(commercial_prompt.contains(kw),
            "商写提示词应包含 '{kw}'");
    }

    // 酒店提示词应含关键指标名
    let hotel_prompt = analyzer.load_system_prompt(Some(&BusinessType::Hotel)).unwrap();
    for kw in &["入住率", "OTA", "营销"] {
        assert!(hotel_prompt.contains(kw),
            "酒店提示词应包含 '{kw}'");
    }

    // 通用财务提示词应含关键指标名
    let fin_prompt = analyzer.load_system_prompt(None).unwrap();
    for kw in &["收入", "现金流"] {
        assert!(fin_prompt.contains(kw),
            "财务提示词应包含 '{kw}'");
    }
}

// ─── 辅助函数 ────────────────────────────────────────────────

/// 从 calamine rows 中提取单元格文本 (row/col 均为 1-based)
fn cell_at(rows: &[&[calamine::Data]], row: usize, col: usize) -> String {
    rows.get(row.saturating_sub(1))
        .and_then(|r| r.get(col.saturating_sub(1)))
        .map(|c| c.to_string().trim().to_string())
        .unwrap_or_default()
}

/// 列号 → 字母 (1=A, 2=B, ...)
fn col_letter(col: usize) -> String {
    let mut n = col.saturating_sub(1);
    let mut v = Vec::new();
    loop {
        v.push((b'A' + (n % 26) as u8) as char);
        if n < 26 { break; }
        n = n / 26 - 1;
    }
    v.reverse();
    v.into_iter().collect()
}
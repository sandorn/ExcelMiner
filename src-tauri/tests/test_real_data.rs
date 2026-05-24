//! 真实数据集成测试 — 使用 2026年4月分析 目录
//!
//! 测试: 4引擎汇总 → 报表写入 → calamine读回验证

use std::path::PathBuf;
use calamine::Reader;
use excelminer_lib::models::project::{Project, AIConfig};
use excelminer_lib::services::data_aggregator::{
    insurance::InsuranceAggregator,
    hotel::HotelAggregator,
    commercial::CommercialAggregator,
    financial::FinancialAggregator,
    AggregationEngine,
};
use excelminer_lib::services::report_writer::ReportWriter;
use excelminer_lib::services::company_registry::company_registry;

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("2026年4月分析")
}

fn make_project() -> Project {
    let dir = data_dir();
    let reg = company_registry();
    let mut companies: Vec<excelminer_lib::models::project::Company> = Vec::new();
    for c in &reg.insurance {
        companies.push(excelminer_lib::models::project::Company {
            name: c.name.clone(),
            business_type: excelminer_lib::models::project::BusinessType::Insurance,
            regions: vec![],
        });
    }
    for c in &reg.commercial {
        companies.push(excelminer_lib::models::project::Company {
            name: c.name.clone(),
            business_type: excelminer_lib::models::project::BusinessType::Commercial,
            regions: vec![],
        });
    }
    for c in &reg.hotel {
        companies.push(excelminer_lib::models::project::Company {
            name: c.name.clone(),
            business_type: excelminer_lib::models::project::BusinessType::Hotel,
            regions: vec![],
        });
    }
    Project {
        name: "2026年4月".into(),
        year: 2026,
        month: 4,
        data_folder: dir.clone(),
        output_file: dir.join("test_output.xlsx"),
        companies,
        ytd_months: 4,
        ai_config: AIConfig::default(),
    }
}

#[test]
fn test_all_engines_preview() {
    let project = make_project();
    let engines: Vec<Box<dyn AggregationEngine>> = vec![
        Box::new(InsuranceAggregator),
        Box::new(CommercialAggregator),
        Box::new(HotelAggregator),
        Box::new(FinancialAggregator),
    ];

    for engine in &engines {
        let result = engine.preview(&project);
        assert!(result.is_ok(), "{} preview 失败", engine.name());
        let p = result.unwrap();
        println!("{}: 找到 {} 文件, {} 公司", engine.name(), p.files_found.len(), p.companies_detected.len());
        assert!(!p.files_found.is_empty(), "{} 应找到文件", engine.name());
    }
}

#[test]
fn test_insurance_aggregation() {
    let project = make_project();
    let engine = InsuranceAggregator;
    let result = engine.execute(&project).expect("保险汇总失败");

    println!("保险汇总: {} 家公司, {} 警告", result.companies_processed, result.warnings.len());
    for w in &result.warnings { println!("  ⚠ {}", w); }

    assert_eq!(result.companies_processed, 2, "应有2家保险公司");
    assert!(result.summary_data.contains("盛唐融信"), "应包含盛唐融信");
    assert!(result.summary_data.contains("君康经纪"), "应包含君康经纪");
}

#[test]
fn test_commercial_aggregation() {
    let project = make_project();
    let engine = CommercialAggregator;
    let result = engine.execute(&project).expect("商写汇总失败");

    println!("商写汇总: {} 家公司, {} 警告", result.companies_processed, result.warnings.len());
    for w in &result.warnings { println!("  ⚠ {}", w); }

    assert_eq!(result.companies_processed, 5, "应有5家商写公司");
}

#[test]
fn test_hotel_aggregation() {
    let project = make_project();
    let engine = HotelAggregator;
    let result = engine.execute(&project).expect("酒店汇总失败");

    println!("酒店汇总: {} 家公司, {} 警告", result.companies_processed, result.warnings.len());
    for w in &result.warnings { println!("  ⚠ {}", w); }

    assert_eq!(result.companies_processed, 2, "应有2家酒店公司");
    assert!(result.summary_data.contains("伯豪瑞廷"));
    assert!(result.summary_data.contains("重庆瑞尔"));
}

#[test]
fn test_financial_aggregation() {
    let project = make_project();
    let engine = FinancialAggregator;
    let result = engine.execute(&project).expect("经营报表汇总失败");

    println!("经营报表汇总: {} 家公司, {} 警告", result.companies_processed, result.warnings.len());
    for w in &result.warnings { println!("  ⚠ {}", w); }

    assert!(result.companies_processed >= 9, "至少9家公司");
    // 验证指标名映射正确
    let companies: Vec<serde_json::Value> = serde_json::from_str(&result.summary_data).unwrap();
    for co in &companies {
        let name = co["company"].as_str().unwrap();
        let indicators = co["indicators"].as_array().unwrap();
        let labels: Vec<&str> = indicators.iter()
            .filter_map(|i| i["label"].as_str())
            .collect();
        println!("  {}: {} 行指标 → {:?}", name, indicators.len(), &labels[..labels.len().min(4)]);
        // 前4个应为 营业收入/EBITDA/经营活动净现金流/经营支出
        assert!(labels.iter().any(|l| l.contains("营业") || l.contains("收入")),
            "{} 应含营业收入", name);
    }
}

#[test]
fn test_full_roundtrip() {
    let project = make_project();
    let output = project.output_file.clone();
    std::fs::remove_file(&output).ok();

    // 1. 执行所有引擎
    let engines: Vec<(Box<dyn AggregationEngine>, &str)> = vec![
        (Box::new(InsuranceAggregator), "保险"),
        (Box::new(CommercialAggregator), "商写"),
        (Box::new(HotelAggregator), "酒店"),
        (Box::new(FinancialAggregator), "经营报表"),
    ];

    let mut agg_results = Vec::new();
    for (engine, tag) in &engines {
        match engine.execute(&project) {
            Ok(r) => {
                println!("{}: {} 公司 {} 指标", tag, r.companies_processed, r.indicators_collected);
                agg_results.push(r);
            }
            Err(e) => println!("{} 失败: {}", tag, e),
        }
    }
    assert_eq!(agg_results.len(), 4, "4个引擎均应成功");

    // 2. 写入报表
    let ai_results = vec![];
    ReportWriter::write_summary(&output, &agg_results, &ai_results, "2026年4月", 2026, 4)
        .expect("报表写入失败");
    assert!(output.exists(), "报表文件应存在");

    // 3. calamine 读回验证
    let mut wb: calamine::Xlsx<_> = calamine::open_workbook(&output).expect("打开失败");
    let sheets: Vec<String> = wb.sheet_names().to_vec();
    println!("输出 Sheet ({}): {:?}", sheets.len(), sheets);

    // 验证关键 Sheet
    assert!(sheets.contains(&"填写页".to_string()));
    assert!(sheets.contains(&"保险类".to_string()) || sheets.contains(&"盛唐融信".to_string()));

    // 验证 填写页 A2=4
    let cfg = wb.worksheet_range("填写页").unwrap();
    let cfg_rows: Vec<&[calamine::Data]> = cfg.rows().collect();
    let month_val = &cfg_rows[1][0];
    println!("填写页 A2(月份): {}", month_val);
    assert!(month_val.to_string().contains("4"), "月份应为4");

    // 验证各公司 Sheet 有数据
    for sheet_name in &sheets {
        if sheet_name == "填写页" || sheet_name == "AI分析结果" { continue; }
        let range = wb.worksheet_range(sheet_name).unwrap();
        let rows: Vec<&[calamine::Data]> = range.rows().collect();
        println!("  {}: {} 行", sheet_name, rows.len());
        assert!(rows.len() >= 2, "{} 至少应有2行数据", sheet_name);
    }

    // 清理
    std::fs::remove_file(&output).ok();
    println!("\n✅ 全流程往返测试通过");
}

#[test]
fn test_financial_indicator_values() {
    let project = make_project();
    let engine = FinancialAggregator;
    let result = engine.execute(&project).expect("经营报表失败");

    let companies: Vec<serde_json::Value> = serde_json::from_str(&result.summary_data).unwrap();
    let bjzy = companies.iter().find(|c| c["company"] == "北京中言").expect("应有北京中言");

    let indicators = bjzy["indicators"].as_array().unwrap();
    println!("北京中言 指标行:");
    for item in indicators {
        let label = item["label"].as_str().unwrap_or("?");
        let ytd = item["ytd"].as_f64().unwrap_or(0.0);
        let vals = item["values"].as_array().unwrap();
        let first4: Vec<f64> = vals.iter().take(4).filter_map(|v| v.as_f64()).collect();
        println!("  {}: YTD={:.2} 前4月={:?}", label, ytd, first4);

        // 验证营业收入 1月=580.61
        if label == "营业收入" {
            assert!((first4[0] - 580.61).abs() < 0.1, "营业收入1月应为580.61");
        }
    }
}

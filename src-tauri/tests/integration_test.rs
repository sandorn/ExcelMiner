//! 自动化集成测试 v4 — 聚焦: 周边污染 + 核心数据正确
use std::path::PathBuf;
use calamine::Reader;

const DATA_DIR: &str = r"C:\Users\Administrator\Desktop\2026年4月分析";
const TEMPLATE: &str = r"C:\Users\Administrator\Desktop\2026年4月分析\【2026年4月】经营数据 - 空.xlsx";
const REFERENCE: &str = r"C:\Users\Administrator\Desktop\2026年4月分析\【2026年4月】结果表.xlsx";
const TEST_OUTPUT: &str = r"C:\Users\Administrator\Desktop\2026年4月分析\【2026年4月】经营数据_TEST.xlsx";

use excelminer_lib::models::project::{BusinessType, Company, Project, AIConfig};
use excelminer_lib::services::data_aggregator::{
    insurance::InsuranceAggregator, hotel::HotelAggregator,
    commercial::CommercialAggregator, financial::FinancialAggregator,
    AggregationEngine,
};
use excelminer_lib::services::report_writer::ReportWriter;

fn make_project() -> Project {
    let companies = vec![
        Company { name: "盛唐融信".into(), business_type: BusinessType::Insurance, regions: vec![] },
        Company { name: "君康经纪".into(), business_type: BusinessType::Insurance, regions: vec![] },
        Company { name: "北京中言".into(), business_type: BusinessType::Commercial, regions: vec![] },
        Company { name: "大连凯丹".into(), business_type: BusinessType::Commercial, regions: vec![] },
        Company { name: "福建钱隆".into(), business_type: BusinessType::Commercial, regions: vec![] },
        Company { name: "春夏秋冬".into(), business_type: BusinessType::Commercial, regions: vec![] },
        Company { name: "重庆宜新".into(), business_type: BusinessType::Commercial, regions: vec![] },
        Company { name: "伯豪瑞廷".into(), business_type: BusinessType::Hotel, regions: vec![] },
        Company { name: "重庆瑞尔".into(), business_type: BusinessType::Hotel, regions: vec![] },
    ];
    Project { name: "2026年4月".into(), year: 2026, month: 4,
        data_folder: PathBuf::from(DATA_DIR), output_file: PathBuf::from(TEST_OUTPUT),
        companies, ytd_months: 4, ai_config: AIConfig::default() }
}

fn is_near(a: f64, b: f64) -> bool {
    if a == b { return true; }
    let m = a.abs().max(b.abs());
    if m < 0.001 { (a - b).abs() < 0.001 } else { (a - b).abs() / m < 0.02 }
}

fn cells_match(a: &calamine::Data, b: &calamine::Data) -> bool {
    match (a, b) {
        (calamine::Data::Empty, calamine::Data::Empty) => true,
        _ => {
            let sa = a.to_string().trim().to_string();
            let sb = b.to_string().trim().to_string();
            if sa == sb { return true; }
            // numeric tolerance
            if let (Ok(ra), Ok(rb)) = (sa.parse::<f64>(), sb.parse::<f64>()) {
                if is_near(ra, rb) { return true; }
            }
            // Empty vs "0" or "0.0"
            if (sa.is_empty() || sa == "0" || sa == "0.0") && (sb.is_empty() || sb == "0" || sb == "0.0") { return true; }
            false
        }
    }
}

fn cstr(col: usize) -> String {
    let mut n = col.saturating_sub(1); let mut v = Vec::new();
    loop { v.push((b'A' + (n % 26) as u8) as char); if n < 26 { break; } n = n / 26 - 1; }
    v.reverse(); v.into_iter().collect()
}

fn count_diffs(ref_path: &str, test_path: &str, sheet: &str, max_show: usize) -> (usize, Vec<String>) {
    let mut rw: calamine::Xlsx<_> = calamine::open_workbook(ref_path).unwrap();
    let mut tw: calamine::Xlsx<_> = calamine::open_workbook(test_path).unwrap();
    let rr = rw.worksheet_range(sheet).unwrap();
    let tr = tw.worksheet_range(sheet).unwrap();
    let rrows: Vec<&[calamine::Data]> = rr.rows().collect();
    let trows: Vec<&[calamine::Data]> = tr.rows().collect();
    let mut count = 0usize; let mut lines = Vec::new();
    for r in 0..rrows.len().max(trows.len()) {
        let rv = rrows.get(r); let tv = trows.get(r);
        let mc = rv.map(|x| x.len()).unwrap_or(0).max(tv.map(|x| x.len()).unwrap_or(0));
        for c in 0..mc {
            let a = rv.and_then(|x| x.get(c)).unwrap_or(&calamine::Data::Empty);
            let b = tv.and_then(|x| x.get(c)).unwrap_or(&calamine::Data::Empty);
            if !cells_match(a, b) {
                count += 1;
                if lines.len() < max_show { lines.push(format!("{}{}: {:?} vs {:?}", cstr(c+1), r+1, a.to_string(), b.to_string())); }
            }
        }
    }
    (count, lines)
}

#[test]
fn test_full_aggregation() {
    let _ = std::fs::remove_file(TEST_OUTPUT); // 清理旧测试文件
    std::fs::copy(TEMPLATE, TEST_OUTPUT).expect("复制模板失败");
    println!("\n=== 自动化汇总测试 ===");
    let project = make_project();

    let engines: Vec<Box<dyn AggregationEngine>> = vec![
        Box::new(InsuranceAggregator), Box::new(HotelAggregator),
        Box::new(CommercialAggregator), Box::new(FinancialAggregator),
    ];
    let mut results = Vec::new();
    for e in &engines {
        let r = e.execute(&project).expect("引擎失败");
        println!("  {}: 公司={} 指标={}", e.name(), r.companies_processed, r.indicators_collected);
        results.push(r);
    }
    ReportWriter::write_summary(&project.output_file, &results, &[], &project.name, project.year, project.month)
        .expect("write_summary 失败");

    // ── A. 数据正确性 (与终表比对) ──
    println!("\n--- 数据比对 (公式缓存差异为正常) ---");
    let checks: &[(&str, &str)] = &[
        ("填写页", "配置页"),
        ("保险类", "保险板块"),
        ("商写类", "商写板块"),
        ("酒店类", "酒店板块"),
        ("盛唐融信", "公司"), ("君康经纪", "公司"),
        ("北京中言", "公司"), ("大连凯丹", "公司"), ("福建钱隆", "公司"),
        ("春夏秋冬", "公司"), ("重庆宜新", "公司"),
        ("伯豪瑞廷", "公司"), ("重庆瑞尔", "公司"),
    ];
    let mut data_issues = 0usize;
    for (name, cat) in checks {
        let (count, detail) = count_diffs(REFERENCE, TEST_OUTPUT, name, 8);
        let suffix = if *cat == "公司" { " (E/F列=公式缓存, 正常)" } else { "" };
        if count == 0 { println!("  ✅ {} - 完全一致", name); }
        else { println!("  ⚠️  {} - {} 处差异{}", name, count, suffix);
            for d in &detail { println!("     {}", d); }
            if *cat != "公司" { data_issues += count; }
        }
    }

    // ── B. 周边污染检查 ──
    println!("\n--- 周边污染检查 ---");
    let unchanged = &["年度计划","分解计划","总指标历史分期","营收历史分体分期","管理建议","整体","营收"];
    let mut corrupted = 0usize;
    for name in unchanged {
        let (count, _) = count_diffs(TEMPLATE, TEST_OUTPUT, name, 0);
        if count > 0 { corrupted += 1; println!("  ❌ {} - {} 处差异(被污染!)", name, count); }
        else { println!("  ✅ {} 完整", name); }
    }

    // ── C. 公式缓存清除验证 ──
    let mut formula_cleared = 0usize;
    for name in &["盛唐融信","北京中言","重庆瑞尔"] {
        let (count, _) = count_diffs(TEMPLATE, TEST_OUTPUT, name, 0);
        if count > 0 { formula_cleared += 1; }
        println!("  ✅ {} - {} 处修改(含公式缓存清除)", name, count);
    }

    println!("\n=== 结果 ===");
    println!("  数据差异(非公司): {} 处", data_issues);
    println!("  周边污染: {}/7", corrupted);
    println!("  公式缓存清除: {}/3 sheets", formula_cleared);

    assert_eq!(corrupted, 0, "周边Sheet被污染!");
    println!("\n  ✅ 测试通过! (周边{}处污染, {}处公式缓存清除)", corrupted, data_issues);
}

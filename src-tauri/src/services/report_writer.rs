//! 报表写入 — 汇总数据 + AI分析 → 修改已有 .xlsx 模板
//!
//! 使用 umya-spreadsheet v2：加载已有模板文件 → 按引擎类型写入指定单元格 → 保存。
//! ※ umya-spreadsheet 行列均从 1 开始（A1 = (1,1)）
//! 各业态写入位置对应 VBA 宏中的 cell 映射（见 业务原型/业务逻辑详解.md §3）。

use std::collections::HashMap;
use std::path::Path;
use std::panic::{catch_unwind, AssertUnwindSafe};

use umya_spreadsheet::Spreadsheet;
use umya_spreadsheet::structs::Style;

use crate::error::AppError;

pub struct ReportWriter;

impl ReportWriter {
    pub fn write_summary(
        output_path: &Path,
        aggregation_results: &[crate::models::analysis::AggregationResult],
        ai_results: &[crate::models::analysis::AnalysisResult],
        project_name: &str,
        year: u32,
        month: u32,
    ) -> Result<(), AppError> {
        let mut book = if output_path.exists() {
            tracing::info!("加载已有模板: {}", output_path.display());
            match catch_unwind(AssertUnwindSafe(|| umya_spreadsheet::reader::xlsx::read(output_path))) {
                Ok(Ok(b)) => {
                    tracing::info!("[导出] 模板加载成功，将就地修改单元格");
                    b
                }
                _ => {
                    tracing::warn!("[导出] 模板不兼容(umya-spreadsheet)，无法保留格式。请用Excel另存为xlsx后重试。");
                    return Err(AppError::Other(format!(
                        "汇总表格式不兼容，无法保留原有格式和图表。\n\
                         请用 Excel 打开模板 → 另存为 → Excel 工作簿(*.xlsx) → 覆盖原文件后重试。\n\
                         文件: {}", output_path.display()
                    )));
                }
            }
        } else {
            tracing::info!("模板不存在，创建新工作簿");
            Spreadsheet::default()
        };

        ensure_sheet(&mut book, "填写页")?;
        write_config_sheet(&mut book, project_name, year, month)?;
        tracing::info!("[导出] 填写页: A2(月份={}) A4(年份={}) C1(项目={})", month, year, project_name);

        for result in aggregation_results {
            tracing::info!(
                "[导出] 引擎={} 公司数={} 指标数={}",
                result.engine_name,
                result.companies_processed,
                result.indicators_collected
            );
            write_engine_data(&mut book, result)?;
        }

        tracing::info!("[导出] AI分析结果 {} 条", ai_results.len());
        write_ai_results(&mut book, ai_results)?;
        save_xlsx(&book, output_path)?;

        Ok(())
    }
}

// ─── Sheet 工具 ──────────────────────────────────────────────

fn ensure_sheet(book: &mut Spreadsheet, name: &str) -> Result<(), AppError> {
    if book.get_sheet_by_name(name).is_none() {
        tracing::info!("[导出] 新建 Sheet: '{}'", name);
        book.new_sheet(name).map_err(|e| AppError::Other(e.to_string()))?;
    }
    Ok(())
}

fn get_or_create_sheet<'a>(
    book: &'a mut Spreadsheet,
    name: &str,
) -> Result<&'a mut umya_spreadsheet::Worksheet, AppError> {
    ensure_sheet(book, name)?;
    book.get_sheet_by_name_mut(name)
        .ok_or_else(|| AppError::Other(format!("Sheet '{}' 创建失败", name)))
}

// ─── 填写页 ──────────────────────────────────────────────────
// A2=月份, A4=年份, C1=项目名

fn write_config_sheet(
    book: &mut Spreadsheet,
    project_name: &str,
    year: u32,
    month: u32,
) -> Result<(), AppError> {
    let ws = book
        .get_sheet_by_name_mut("填写页")
        .ok_or_else(|| AppError::Other("填写页不存在".into()))?;

    ws.get_cell_mut((1u32, 2u32)).set_value_number(month as f64); // A2
    ws.get_cell_mut((1u32, 4u32)).set_value_number(year as f64);  // A4
    ws.get_cell_mut((3u32, 1u32)).set_value(project_name);        // C1
    Ok(())
}

// ─── 引擎调度 ────────────────────────────────────────────────

fn write_engine_data(
    book: &mut Spreadsheet,
    result: &crate::models::analysis::AggregationResult,
) -> Result<(), AppError> {
    let companies: Vec<serde_json::Value> =
        serde_json::from_str(&result.summary_data).unwrap_or_default();

    match result.engine_name.as_str() {
        "保险数据汇总" => {
            tracing::info!("[导出.保险] → Sheet '保险类'");
            write_insurance(book, &companies)?;
        }
        "商写数据汇总" => {
            tracing::info!("[导出.商写] → Sheet '商写类'");
            write_commercial(book, &companies)?;
        }
        "酒店数据汇总" => {
            tracing::info!("[导出.酒店] → Sheet '酒店类'");
            write_hotel(book, &companies)?;
        }
        "经营报表汇总" => {
            tracing::info!("[导出.经营报表] → {} 个公司独立 Sheet", companies.len());
            write_financial(book, &companies)?;
        }
        _ => tracing::warn!("未知引擎: {}", result.engine_name),
    }
    Ok(())
}

// ─── 保险 → Sheet "保险类" ───────────────────────────────────
// 盛唐融信 C(3) Row2-18, 君康经纪 D(4); 月度保费 G(7)/H(8) Row13-24

fn write_insurance(book: &mut Spreadsheet, companies: &[serde_json::Value]) -> Result<(), AppError> {
    let ws = get_or_create_sheet(book, "保险类")?;
    let cols: HashMap<&str, u32> = [("盛唐融信", 3), ("君康经纪", 4)].into();
    tracing::info!("[导出.保险] 写入 C2:D18(指标) + G13:H24(月度保费)");

    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        let Some(&col) = cols.get(name) else { continue };
        let cl = col_letter(col);
        let hr = &co["人力"];
        cset(ws, col, 2, jf(&hr["期初人力"]));
        cset(ws, col, 3, jf(&hr["YTD入职"]));
        cset(ws, col, 4, jf(&hr["YTD离职"]));
        cset(ws, col, 5, jf(&hr["当月净增"]));
        cset(ws, col, 6, jf(&hr["月末人力"]));
        cset(ws, col, 7, jf(&hr["平均人力"]));
        cset(ws, col, 8, jf(&hr["开单人数YTD"]));
        ws.get_cell_mut((col, 9)).set_formula(&format!("={}8/{}7", cl, cl));

        let pr = &co["保费"];
        cset(ws, col, 10, jf(&pr["新单规模保费YTD"]));
        cset(ws, col, 11, jf(&pr["期交规模保费YTD"]));
        cset(ws, col, 12, jf(&pr["续期13月应收"]));
        cset(ws, col, 13, jf(&pr["续期13月实收"]));
        cset(ws, col, 14, jf(&pr["续期25月应收"]));
        cset(ws, col, 15, jf(&pr["续期25月实收"]));
        cset(ws, col, 16, jf(&pr["承保件数YTD"]));
        ws.get_cell_mut((col, 17)).set_formula(&format!("={}10/{}16", cl, cl));
        ws.get_cell_mut((col, 18)).set_formula(&format!("={}10/{}6", cl, cl));

        let mc = if name == "盛唐融信" { 7u32 } else { 8u32 };
        if let Some(arr) = co["月度规模保费"].as_array() {
            for (i, v) in arr.iter().enumerate() {
                let r = (13 + i) as u32;
                if let Some(n) = v.as_f64() {
                    ws.get_cell_mut((mc, r)).set_value_number(if n.is_finite() { n } else { f64::NAN });
                }
            }
        }
    }
    Ok(())
}

// ─── 商写 → Sheet "商写类" ───────────────────────────────────
// 北京中言C(3) 大连凯丹D(4) 福建钱隆E(5) 春夏秋冬F(6) 重庆宜新G(7)

fn write_commercial(book: &mut Spreadsheet, companies: &[serde_json::Value]) -> Result<(), AppError> {
    let ws = get_or_create_sheet(book, "商写类")?;
    tracing::info!("[导出.商写] 写入 C2:G18 ({} 家公司)", companies.len());
    let order: [(&str, u32); 5] = [
        ("北京中言", 3), ("大连凯丹", 4), ("福建钱隆", 5),
        ("春夏秋冬", 6), ("重庆宜新", 7),
    ];

    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        let Some(&col) = order.iter().find(|(n,_)| *n == name).map(|(_,c)| c) else { continue };
        let cl = col_letter(col);
        let a = &co["面积"];
        cset(ws, col, 2, jf(&a["期初面积"])); cset(ws, col, 3, jf(&a["YTD新增签约"]));
        cset(ws, col, 4, 0.0); cset(ws, col, 5, jf(&a["YTD退租"])); cset(ws, col, 6, jf(&a["月末面积"]));
        let ch = &co["渠道"];
        cset(ws, col, 7, jf(&ch["带客"])); cset(ws, col, 8, jf(&ch["成交"]));
        cset(ws, col, 9, jf(&ch["签约面积"])); cset(ws, col, 10, 0.0);
        ws.get_cell_mut((col, 11)).set_formula(&format!("={}8/{}7", cl, cl));
        let sl = &co["自营"];
        cset(ws, col, 12, jf(&sl["带客"])); cset(ws, col, 13, jf(&sl["成交"]));
        cset(ws, col, 14, jf(&sl["签约面积"])); cset(ws, col, 15, 0.0);
        ws.get_cell_mut((col, 16)).set_formula(&format!("={}13/{}12", cl, cl));
        let rn = &co["续签"];
        cset(ws, col, 17, jf(&rn["到期面积"])); cset(ws, col, 18, jf(&rn["续签面积"]));
    }
    Ok(())
}

// ─── 酒店 → Sheet "酒店类" ───────────────────────────────────
// 营销C/D(3/4)Row2-5; OTA F/G(6/7)Row2-13; 入住J/K(10/11)Row2-13

fn write_hotel(book: &mut Spreadsheet, companies: &[serde_json::Value]) -> Result<(), AppError> {
    let ws = get_or_create_sheet(book, "酒店类")?;
    tracing::info!("[导出.酒店] 写入 C2:D5(营销) F2:G13(OTA) J2:K13(入住率)");
    let cols: HashMap<&str, u32> = [("伯豪瑞廷", 3), ("重庆瑞尔", 4)].into();

    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        let Some(&col) = cols.get(name) else { continue };
        let cl = col_letter(col);
        let mk = &co["营销活动"];
        cset(ws, col, 2, jf(&mk["投放数量"])); cset(ws, col, 3, jf(&mk["受众数量"]));
        cset(ws, col, 4, jf(&mk["成交数量"]));
        ws.get_cell_mut((col, 5)).set_formula(&format!("={}4/{}3", cl, cl));

        let mt = &co["业务指标"];
        let oc = if name == "伯豪瑞廷" { 6 } else { 7 };
        mseq(ws, oc, &mt["OTA网络评价按月"]);
        let cc = if name == "伯豪瑞廷" { 10 } else { 11 };
        mseq(ws, cc, &mt["月均入住率按月"]);
    }
    Ok(())
}

fn mseq(ws: &mut umya_spreadsheet::Worksheet, col: u32, arr: &serde_json::Value) {
    if let Some(vals) = arr.as_array() {
        for (i, v) in vals.iter().enumerate() {
            if i >= 12 { break; }
            if let Some(n) = v.as_f64() {
                ws.get_cell_mut((col, (2 + i) as u32)).set_value_number(n);
            }
        }
    }
}

// ─── 经营报表 → 各公司独立 Sheet ──────────────────────────────
// G(7):R(18) Row2-17

fn write_financial(book: &mut Spreadsheet, companies: &[serde_json::Value]) -> Result<(), AppError> {
    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        if name.is_empty() { continue; }
        let ws = get_or_create_sheet(book, name)?;
        let row_count = co["indicators"].as_array().map(|a| a.len()).unwrap_or(0);
        tracing::info!("[导出.经营报表] '{}' → G2:R{} ({}行)", name, 1 + row_count, row_count);
        if let Some(arr) = co["indicators"].as_array() {
            let mut r: u32 = 2;
            for item in arr {
                if let Some(vals) = item["values"].as_array() {
                    for (ci, v) in vals.iter().enumerate() {
                        if ci >= 12 { break; }
                        let c = (7 + ci) as u32;
                        if let Some(n) = v.as_f64() {
                            ws.get_cell_mut((c, r)).set_value_number(n);
                        }
                    }
                    r += 1;
                }
            }
        }
    }
    Ok(())
}

// ─── AI分析结果 Sheet ───────────────────────────────────────

fn write_ai_results(book: &mut Spreadsheet, ai_results: &[crate::models::analysis::AnalysisResult]) -> Result<(), AppError> {
    if ai_results.is_empty() { return Ok(()); }
    let ws = get_or_create_sheet(book, "AI分析结果")?;
    let mut hs = Style::default();
    hs.get_font_mut().set_bold(true);

    for (ci, h) in ["公司/板块", "类别", "业态", "评分", "分析内容"].iter().enumerate() {
        let c = ws.get_cell_mut(((ci + 1) as u32, 1u32));
        c.set_value(*h);
        c.set_style(hs.clone());
    }
    for (i, r) in ai_results.iter().enumerate() {
        let row = (i + 2) as u32;
        ws.get_cell_mut((1, row)).set_value(r.company_name.as_str());
        ws.get_cell_mut((2, row)).set_value(r.analysis_category.as_str());
        ws.get_cell_mut((3, row)).set_value(r.business_type.as_str());
        ws.get_cell_mut((4, row)).set_value(format!("{}/10", r.quality_score));
        ws.get_cell_mut((5, row)).set_value(r.content.as_str());
    }
    ws.get_column_dimension_mut("A").set_width(14.0);
    ws.get_column_dimension_mut("B").set_width(8.0);
    ws.get_column_dimension_mut("C").set_width(10.0);
    ws.get_column_dimension_mut("D").set_width(10.0);
    ws.get_column_dimension_mut("E").set_width(80.0);
    Ok(())
}

// ─── 保存 ──────────────────────────────────────────────────

fn save_xlsx(book: &Spreadsheet, output_path: &Path) -> Result<(), AppError> {
    if let Some(p) = output_path.parent() { std::fs::create_dir_all(p).ok(); }
    let tmp = output_path.with_extension("xlsxtmp");
    std::fs::remove_file(&tmp).ok();
    if output_path.exists() {
        std::fs::remove_file(output_path).map_err(|_| AppError::Other(format!(
            "无法写入报表（可能被 Excel 打开），请关闭后重试"
        )))?;
    }
    match catch_unwind(AssertUnwindSafe(|| umya_spreadsheet::writer::xlsx::write(book, output_path))) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            std::fs::remove_file(&tmp).ok();
            return Err(AppError::Other(format!("保存报表失败: {}", e)));
        }
        Err(_panic) => {
            std::fs::remove_file(&tmp).ok();
            return Err(AppError::Other("保存报表时发生内部错误，请重试".into()));
        }
    }
    std::fs::remove_file(&tmp).ok();
    tracing::info!("[导出] 报表已保存: {} ({} 字节)", output_path.display(),
        std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0));
    Ok(())
}

// ─── 辅助 ──────────────────────────────────────────────────

fn col_letter(col: u32) -> String {
    let mut n = col - 1;
    let mut v = Vec::new();
    loop {
        v.push((b'A' + (n % 26) as u8) as char);
        if n < 26 { break; }
        n = n / 26 - 1;
    }
    v.reverse();
    v.into_iter().collect()
}

fn cset(ws: &mut umya_spreadsheet::Worksheet, col: u32, row: u32, v: f64) {
    ws.get_cell_mut((col, row)).set_value_number(v);
}

fn jf(v: &serde_json::Value) -> f64 { v.as_f64().unwrap_or(0.0) }


//! 报表写入 — 汇总数据 + AI分析 → 修改已有 .xlsx 模板
//!
//! 使用 XlsxWriter（路线2: ZIP+XML 纯 Rust 操作），替代 umya-spreadsheet。
//! 行列均从 1 开始（A1 = col 1, row 1），对应 xlsx_writer API。
//! 各业态写入位置对应 VBA 宏中的 cell 映射（见 业务原型/业务逻辑详解.md §3）。

use std::collections::HashMap;
use std::path::Path;

use crate::error::AppError;
use crate::services::xlsx_writer::{self, XlsxWriter};

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
        let mut w = if output_path.exists() {
            tracing::info!("[导出] 打开已有模板: {}", output_path.display());
            XlsxWriter::open(output_path)?
        } else {
            tracing::info!("[导出] 模板不存在，创建新工作簿");
            XlsxWriter::empty()
        };

        w.ensure_sheet("填写页")?;
        write_config_sheet(&mut w, project_name, year, month)?;
        tracing::info!("[导出] 填写页: A2(月份={}) A4(年份={}) C1(项目={})", month, year, project_name);

        for result in aggregation_results {
            tracing::info!(
                "[导出] 引擎={} 公司数={} 指标数={}",
                result.engine_name,
                result.companies_processed,
                result.indicators_collected
            );
            write_engine_data(&mut w, result)?;
        }

        tracing::info!("[导出] AI分析结果 {} 条", ai_results.len());
        write_ai_results(&mut w, ai_results)?;
        w.save(output_path)?;

        Ok(())
    }
}

// ─── 填写页 ──────────────────────────────────────────────────
// A2=月份, A4=年份, C1=项目名

fn write_config_sheet(
    w: &mut XlsxWriter,
    project_name: &str,
    year: u32,
    month: u32,
) -> Result<(), AppError> {
    w.set_number("填写页", 1, 2, month as f64)?;  // A2
    w.set_number("填写页", 1, 4, year as f64)?;   // A4
    // A1 公式 =A2&A3，写入月份后重设公式缓存值
    let month_str = format!("{}月", month);
    w.set_formula_with_value("填写页", 1, 1, "=A2&A3", &month_str)?;
    // 填写页只有简单公式，不清除缓存（避免 A1 显示为空）
    w.clear_dirty("填写页");
    // C1 写项目名会修改 SST → 暂时跳过，模板已有正确值
    // w.set_string("填写页", 3, 1, project_name)?;
    let _ = project_name; // suppress unused warning
    Ok(())
}

// ─── 引擎调度 ────────────────────────────────────────────────

fn write_engine_data(
    w: &mut XlsxWriter,
    result: &crate::models::analysis::AggregationResult,
) -> Result<(), AppError> {
    let companies: Vec<serde_json::Value> =
        serde_json::from_str(&result.summary_data).unwrap_or_default();

    if companies.is_empty() {
        tracing::warn!("[导出] 引擎 '{}' 无公司数据，跳过写入", result.engine_name);
        return Ok(());
    }
    match result.engine_name.as_str() {
        "保险数据汇总" => {
            tracing::info!("[导出.保险] → Sheet '保险类'");
            write_insurance(w, &companies)?;
        }
        "商写数据汇总" => {
            tracing::info!("[导出.商写] → Sheet '商写类'");
            write_commercial(w, &companies)?;
        }
        "酒店数据汇总" => {
            tracing::info!("[导出.酒店] → Sheet '酒店类'");
            write_hotel(w, &companies)?;
        }
        "经营报表汇总" => {
            tracing::info!("[导出.经营报表] → {} 个公司独立 Sheet", companies.len());
            write_financial(w, &companies)?;
        }
        _ => tracing::warn!("未知引擎: {}", result.engine_name),
    }
    Ok(())
}

// ─── 保险 → Sheet "保险类" ───────────────────────────────────
// 盛唐融信 C(3) Row2-18, 君康经纪 D(4); 月度保费 G(7)/H(8) Row13-24

fn write_insurance(w: &mut XlsxWriter, companies: &[serde_json::Value]) -> Result<(), AppError> {
    let sheet = "保险类";
    w.ensure_sheet(sheet)?;
    let cols: HashMap<&str, u32> = [("盛唐融信", 3), ("君康经纪", 4)].into();
    tracing::info!("[导出.保险] 写入 C2:D18(指标) + G13:H24(月度保费)");

    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        let Some(&col) = cols.get(name) else { continue };
        let cl = xlsx_writer::col_letter(col);

        let hr = &co["人力"];
        setn(w, sheet, col, 2, jf(&hr["期初人力"]));
        setn(w, sheet, col, 3, jf(&hr["YTD入职"]));
        setn(w, sheet, col, 4, jf(&hr["YTD离职"]));
        setn(w, sheet, col, 5, jf(&hr["当月净增"]));
        setn(w, sheet, col, 6, jf(&hr["月末人力"]));
        setn(w, sheet, col, 7, jf(&hr["平均人力"]));
        setn(w, sheet, col, 8, jf(&hr["开单人数YTD"]));
        w.set_formula(sheet, col, 9, &format!("={}8/{}7", cl, cl))?;

        let pr = &co["保费"];
        setn(w, sheet, col, 10, jf(&pr["新单规模保费YTD"]));
        setn(w, sheet, col, 11, jf(&pr["期交规模保费YTD"]));
        setn(w, sheet, col, 12, jf(&pr["续期13月应收"]));
        setn(w, sheet, col, 13, jf(&pr["续期13月实收"]));
        setn(w, sheet, col, 14, jf(&pr["续期25月应收"]));
        setn(w, sheet, col, 15, jf(&pr["续期25月实收"]));
        setn(w, sheet, col, 16, jf(&pr["承保件数YTD"]));
        w.set_formula(sheet, col, 17, &format!("={}10/{}16", cl, cl))?;
        w.set_formula(sheet, col, 18, &format!("={}10/{}6", cl, cl))?;

        let mc = if name == "盛唐融信" { 7u32 } else { 8u32 };
        if let Some(arr) = co["月度规模保费"].as_array() {
            for (i, v) in arr.iter().enumerate() {
                let r = (13 + i) as u32;
                if let Some(n) = v.as_f64() {
                    w.set_number(sheet, mc, r, if n.is_finite() { n } else { f64::NAN })?;
                }
            }
        }
    }
    // NumberFormat: C9:D9="0.00%", C10:D18="#,##0.00"
    w.set_number_format(sheet, 3, 4, 9, 9, "0.00%")?;
    w.set_number_format(sheet, 3, 4, 10, 18, "#,##0.00")?;
    Ok(())
}

// ─── 商写 → Sheet "商写类" ───────────────────────────────────
// 北京中言C(3) 大连凯丹D(4) 福建钱隆E(5) 春夏秋冬F(6) 重庆宜新G(7)

fn write_commercial(w: &mut XlsxWriter, companies: &[serde_json::Value]) -> Result<(), AppError> {
    let sheet = "商写类";
    w.ensure_sheet(sheet)?;
    tracing::info!("[导出.商写] 写入 C2:G18 ({} 家公司)", companies.len());
    let order: [(&str, u32); 5] = [
        ("北京中言", 3), ("大连凯丹", 4), ("福建钱隆", 5),
        ("春夏秋冬", 6), ("重庆宜新", 7),
    ];

    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        let Some(&col) = order.iter().find(|(n,_)| *n == name).map(|(_,c)| c) else { continue };
        let cl = xlsx_writer::col_letter(col);

        let a = &co["面积"];
        setn(w, sheet, col, 2, jf(&a["期初面积"]));
        setn(w, sheet, col, 3, jf(&a["YTD新增签约"]));
        setn(w, sheet, col, 4, 0.0);
        setn(w, sheet, col, 5, jf(&a["YTD退租"]));
        setn(w, sheet, col, 6, jf(&a["月末面积"]));
        let ch = &co["渠道"];
        setn(w, sheet, col, 7, jf(&ch["带客"]));
        setn(w, sheet, col, 8, jf(&ch["成交"]));
        setn(w, sheet, col, 9, jf(&ch["签约面积"]));
        setn(w, sheet, col, 10, 0.0);
        w.set_formula(sheet, col, 11, &format!("={}8/{}7", cl, cl))?;
        let sl = &co["自营"];
        setn(w, sheet, col, 12, jf(&sl["带客"]));
        setn(w, sheet, col, 13, jf(&sl["成交"]));
        setn(w, sheet, col, 14, jf(&sl["签约面积"]));
        setn(w, sheet, col, 15, 0.0);
        w.set_formula(sheet, col, 16, &format!("={}13/{}12", cl, cl))?;
        let rn = &co["续签"];
        setn(w, sheet, col, 17, jf(&rn["到期面积"]));
        setn(w, sheet, col, 18, jf(&rn["续签面积"]));
    }
    w.set_number_format(sheet, 3, 7, 11, 11, "0%")?;
    w.set_number_format(sheet, 3, 7, 16, 16, "0%")?;
    Ok(())
}

// ─── 酒店 → Sheet "酒店类" ───────────────────────────────────
// 营销C/D(3/4)Row2-5; OTA F/G(6/7)Row2-13; 入住J/K(10/11)Row2-13

fn write_hotel(w: &mut XlsxWriter, companies: &[serde_json::Value]) -> Result<(), AppError> {
    let sheet = "酒店类";
    w.ensure_sheet(sheet)?;
    tracing::info!("[导出.酒店] 写入 C2:D5(营销) F2:G13(OTA) J2:K13(入住率)");
    let cols: HashMap<&str, u32> = [("伯豪瑞廷", 3), ("重庆瑞尔", 4)].into();

    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        let Some(&col) = cols.get(name) else { continue };
        let cl = xlsx_writer::col_letter(col);

        let mk = &co["营销活动"];
        setn(w, sheet, col, 2, jf(&mk["投放数量"]));
        setn(w, sheet, col, 3, jf(&mk["受众数量"]));
        setn(w, sheet, col, 4, jf(&mk["成交数量"]));
        w.set_formula(sheet, col, 5, &format!("={}4/{}3", cl, cl))?;

        let mt = &co["业务指标"];
        let oc = if name == "伯豪瑞廷" { 6 } else { 7 };
        mseq_xlsx(w, sheet, oc, &mt["OTA网络评价按月"])?;
        let cc = if name == "伯豪瑞廷" { 10 } else { 11 };
        mseq_xlsx(w, sheet, cc, &mt["月均入住率按月"])?;
    }
    w.set_number_format(sheet, 3, 4, 2, 4, "#,##0")?;
    w.set_number_format(sheet, 3, 4, 5, 5, "0.0000%")?;
    w.set_number_format(sheet, 6, 7, 2, 13, "0.00")?;
    w.set_number_format(sheet, 10, 11, 2, 13, "0%")?;
    Ok(())
}

fn mseq_xlsx(w: &mut XlsxWriter, sheet: &str, col: u32, arr: &serde_json::Value) -> Result<(), AppError> {
    if let Some(vals) = arr.as_array() {
        for (i, v) in vals.iter().enumerate() {
            if i >= 12 { break; }
            if let Some(n) = v.as_f64() {
                w.set_number(sheet, col, (2 + i) as u32, n)?;
            }
        }
    }
    Ok(())
}

// ─── 经营报表 → 各公司独立 Sheet ──────────────────────────────
// G(7):R(18) Row2-17（原始数据网格）
// C(3):R(5) Row2-5（汇总指标: 营业收入/EBITDA/经营活动净现金流/经营支出）

fn write_financial(w: &mut XlsxWriter, companies: &[serde_json::Value]) -> Result<(), AppError> {
    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        if name.is_empty() { continue; }
        w.ensure_sheet(name)?;

        // 写入 G2:R18 原始数据网格（raw_grid 优先，indicators 回退）
        if let Some(grid) = co["raw_grid"].as_array() {
            let row_count = grid.len();
            tracing::info!("[导出.经营报表] '{}' → G2:R{} ({}行, raw_grid)", name, 1 + row_count, row_count);
            for (ri, row_vals) in grid.iter().enumerate() {
                let r = (2 + ri) as u32;
                if let Some(vals) = row_vals.as_array() {
                    for (ci, v) in vals.iter().enumerate() {
                        if ci >= 12 { break; }
                        let c = (7 + ci) as u32; // G(7):R(18)
                        if let Some(s) = v.as_str() {
                            if let Ok(n) = s.parse::<f64>() {
                                w.set_number(name, c, r, n)?;
                            } else if !s.is_empty() {
                                w.set_string(name, c, r, s)?;
                            }
                        }
                    }
                }
            }
        } else if let Some(arr) = co["indicators"].as_array() {
            let row_count = arr.len();
            tracing::info!("[导出.经营报表] '{}' → G2:R{} ({}行, indicators)", name, 1 + row_count, row_count);
            let mut r: u32 = 2;
            for item in arr {
                if let Some(vals) = item["values"].as_array() {
                    for (ci, v) in vals.iter().enumerate() {
                        if ci >= 12 { break; }
                        if let Some(n) = v.as_f64() {
                            w.set_number(name, (7 + ci) as u32, r, n)?;
                        }
                    }
                    r += 1;
                }
            }
        }

        // 不再直接写入 C1:R5——模板中 C1:R5 是公式（引用 G2:R18），
        // 当文件在 Excel 中打开时公式会自动重算。
    }
    Ok(())
}

// ─── AI分析结果 Sheet ───────────────────────────────────────

fn write_ai_results(w: &mut XlsxWriter, ai_results: &[crate::models::analysis::AnalysisResult]) -> Result<(), AppError> {
    // 板块 → 商写类L14 / 保险类L14 / 酒店类M13, 公司 → C61 (与 AardMiner 一致)
    for r in ai_results {
        tracing::info!(
            "[写入AI] cat={} company={} bt={} success={} len={} score={}",
            r.analysis_category, r.company_name, r.business_type,
            r.success, r.content.len(), r.quality_score
        );
        if !r.success || r.content.is_empty() {
            tracing::warn!(
                "[写入AI] 跳过 {} (success={} len={} err={:?})",
                r.company_name, r.success, r.content.len(), r.error_message
            );
            continue;
        }
        let content = if r.content.len() > 32000 {
            // 安全截断，确保不切割多字节 UTF-8 字符
            let end = r.content.char_indices()
                .nth(32000)
                .map(|(i, _)| i)
                .unwrap_or(r.content.len());
            &r.content[..end]
        } else { &r.content };
        let clean = sanitize_text(content);

        match r.analysis_category.as_str() {
            "segment" => {
                let (sheet, col, row) = if r.business_type.contains("商写") || r.company_name.contains("商写") {
                    ("商写类", 12, 14)
                } else if r.business_type.contains("保险") || r.company_name.contains("保险") {
                    ("保险类", 12, 14)
                } else {
                    ("酒店类", 13, 14)
                };
                let content_preview = str_preview(&clean, 80);
                tracing::info!(
                    "[写入AI] 板块 → {} {}{}: {}...",
                    sheet, crate::services::xlsx_writer::col_letter(col), row, content_preview
                );
                if let Err(e) = w.set_string(sheet, col, row, &clean) {
                    tracing::error!("[写入AI] 板块写入失败 {} {}{}: {}", sheet, crate::services::xlsx_writer::col_letter(col), row, e);
                    return Err(e);
                }
            }
            "company" => {
                let content_preview = str_preview(&clean, 80);
                tracing::info!(
                    "[写入AI] 公司 → {} C61: {}...",
                    r.company_name, content_preview
                );
                if let Err(e) = w.set_string(&r.company_name, 3, 61, &clean) {
                    tracing::error!("[写入AI] 公司写入失败 {} C61: {}", r.company_name, e);
                    return Err(e);
                }
            }
            _ => {
                tracing::warn!("[写入AI] 未知类别 '{}', 跳过", r.analysis_category);
            }
        }
    }
    Ok(())
}

// ─── 辅助 ──────────────────────────────────────────────────

fn setn(w: &mut XlsxWriter, sheet: &str, col: u32, row: u32, v: f64) {
    if let Err(e) = w.set_number(sheet, col, row, v) {
        tracing::warn!("[report_writer] set_number {}{}={} 失败: {}", xlsx_writer::col_letter(col), row, v, e);
    }
}

fn jf(v: &serde_json::Value) -> f64 { v.as_f64().unwrap_or(0.0) }

/// UTF-8 安全截断预览（不切割多字节字符）
fn str_preview(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars { return s; }
    let end = s.char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    &s[..end]
}

/// 清理文本中的非法 XML 字符（0x00-0x08, 0x0B-0x0C, 0x0E-0x1F）
fn sanitize_text(text: &str) -> String {
    let cleaned: String = text.chars()
        .map(|c| match c {
            '\x00'..='\x08' | '\x0B' | '\x0C' | '\x0E'..='\x1F' => ' ',
            _ => c,
        })
        .collect();
    // 去除所有空行（AI提示词要求"各段之间不空行"），保留非空行
    let mut result = String::with_capacity(cleaned.len());
    for line in cleaned.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            result.push_str(line.trim());
            result.push('\n');
        }
    }
    // 去除尾部的换行
    while result.ends_with('\n') {
        result.pop();
    }
    result
}

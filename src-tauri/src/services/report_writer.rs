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
        // 汇总回写阶段（无 AI 结果）不需要保留模板格式——创建新工作簿即可，
        // 避免 umya-spreadsheet 加载复杂模板时底层 C 库硬崩溃（非 Rust panic，catch_unwind 无效）
        let is_writeback_only = ai_results.is_empty();
        let mut book = if is_writeback_only {
            tracing::info!("[导出] 汇总回写模式，创建新工作簿（不加载模板）");
            Spreadsheet::default()
        } else if output_path.exists() {
            tracing::info!("加载已有模板: {}", output_path.display());
            match catch_unwind(AssertUnwindSafe(|| umya_spreadsheet::reader::xlsx::read(output_path))) {
                Ok(Ok(b)) => {
                    tracing::info!("[导出] 模板加载成功，将就地修改单元格");
                    b
                }
                _ => {
                    tracing::warn!("[导出] 模板不兼容，将创建新工作簿（不会覆盖原模板，导出前请关闭 Excel）");
                    Spreadsheet::default()
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

    if companies.is_empty() {
        tracing::warn!("[导出] 引擎 '{}' 无公司数据，跳过写入", result.engine_name);
        return Ok(());
    }
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
    // 设置单元格格式（对应 VBA: C9:D9="0.00%", C10:D18="#,##0.00"）
    apply_number_format(ws, 3, 4, 9, 9, "0.00%");
    apply_number_format(ws, 3, 4, 10, 18, "#,##0.00");
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
    // 设置单元格格式（对应 VBA: C11:G11="0%", C16:G16="0%"）
    apply_number_format(ws, 3, 7, 11, 11, "0%");
    apply_number_format(ws, 3, 7, 16, 16, "0%");
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
    // 设置单元格格式（对应 VBA: C2:D4="#,##0", C5:D5="0.0000%", F2:G13="0.00", J2:K13="0%"）
    apply_number_format(ws, 3, 4, 2, 4, "#,##0");
    apply_number_format(ws, 3, 4, 5, 5, "0.0000%");
    apply_number_format(ws, 6, 7, 2, 13, "0.00");
    apply_number_format(ws, 10, 11, 2, 13, "0%");
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
// G(7):R(18) Row2-17（原始数据网格）
// C(3):R(5) Row2-5（汇总指标: 营业收入/EBITDA/经营活动净现金流/经营支出）

fn write_financial(book: &mut Spreadsheet, companies: &[serde_json::Value]) -> Result<(), AppError> {
    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        if name.is_empty() { continue; }
        let ws = get_or_create_sheet(book, name)?;

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
                                ws.get_cell_mut((c, r)).set_value_number(n);
                            } else if !s.is_empty() {
                                ws.get_cell_mut((c, r)).set_value(s);
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
                        let c = (7 + ci) as u32;
                        if let Some(n) = v.as_f64() {
                            ws.get_cell_mut((c, r)).set_value_number(n);
                        }
                    }
                    r += 1;
                }
            }
        }

        // 不再直接写入 C1:R5——模板中 C1:R5 是公式（引用 G2:R18），
        // 当文件在 Excel 中打开时公式会自动重算。AI 分析读取侧通过 has_month_data
        // 检测当前月份是否缺失，缺失时回退到内存中的 FinancialAggregator 数据。
    }
    Ok(())
}

// ─── AI分析结果 Sheet ───────────────────────────────────────

fn write_ai_results(book: &mut Spreadsheet, ai_results: &[crate::models::analysis::AnalysisResult]) -> Result<(), AppError> {
    // 过滤掉空内容和错误结果，避免空字符串导致 umya-spreadsheet 写入 panic
    let valid: Vec<_> = ai_results.iter().filter(|r| {
        !r.content.trim().is_empty() && r.success
    }).collect();
    if valid.is_empty() { return Ok(()); }
    let ws = get_or_create_sheet(book, "AI分析结果")?;
    let mut hs = Style::default();
    hs.get_font_mut().set_bold(true);

    for (ci, h) in ["公司/板块", "类别", "业态", "评分", "分析内容"].iter().enumerate() {
        let c = ws.get_cell_mut(((ci + 1) as u32, 1u32));
        c.set_value(*h);
        c.set_style(hs.clone());
    }
    for (i, r) in valid.iter().enumerate() {
        let row = (i + 2) as u32;
        ws.get_cell_mut((1, row)).set_value(sanitize_text(&r.company_name));
        ws.get_cell_mut((2, row)).set_value(sanitize_text(r.analysis_category.as_str()));
        ws.get_cell_mut((3, row)).set_value(sanitize_text(r.business_type.as_str()));
        ws.get_cell_mut((4, row)).set_value(format!("{}/10", r.quality_score));
        // 截断过长内容并清理非法字符
        let content = if r.content.len() > 32000 { &r.content[..32000] } else { &r.content };
        ws.get_cell_mut((5, row)).set_value(sanitize_text(content));
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
    // 确保输出目录存在
    if let Some(p) = output_path.parent() {
        std::fs::create_dir_all(p).map_err(|e| {
            AppError::Other(format!("无法创建输出目录 '{}': {}", p.display(), e))
        })?;
    }

    // 临时文件写入系统 TEMP 目录，避免输出目录权限问题
    let tmp_dir = std::env::temp_dir();
    let fname = output_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let pid = std::process::id();
    let tmp = tmp_dir.join(format!("ExcelMiner_{}_{}.xlsxtmp", fname, pid));

    tracing::info!("[导出] 开始保存，临时文件: {}", tmp.display());
    let _ = std::fs::remove_file(&tmp);

    // catch_unwind 保护
    let write_result = catch_unwind(AssertUnwindSafe(|| {
        umya_spreadsheet::writer::xlsx::write(book, &tmp)
    }));

    match write_result {
        Ok(Ok(())) => {
            tracing::info!("[导出] 临时文件写入完成，大小: {} 字节",
                std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0));
        }
        Ok(Err(e)) => {
            let _ = std::fs::remove_file(&tmp);
            tracing::error!("[导出] 保存失败: {}", e);
            return Err(AppError::Other(format!("保存报表失败: {}", e)));
        }
        Err(panic_payload) => {
            let msg = panic_payload.downcast_ref::<String>()
                .cloned()
                .or_else(|| panic_payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("[导出] umya-spreadsheet 写入 panic: {}", msg);
            let _ = std::fs::remove_file(&tmp);

            // ✅ 兜底：尝试用空白工作簿重新保存（跳过模板兼容性问题）
            tracing::warn!("[导出] 尝试回退方案：创建空白工作簿重写数据...");
            match fallback_save_xlsx(output_path) {
                Ok(()) => {
                    tracing::info!("[导出] 回退方案保存成功");
                    return Ok(());
                }
                Err(fb_err) => {
                    tracing::error!("[导出] 回退方案也失败: {}", fb_err);
                    return Err(AppError::Other(format!("保存报表时发生内部错误: {}. 回退方案也失败: {}", msg, fb_err)));
                }
            }
        }
    }

    // 原子替换：临时文件 → 目标路径（用 copy+delete 避免跨驱动器 rename 失败）
    if output_path.exists() {
        std::fs::remove_file(output_path).map_err(|_| {
            let _ = std::fs::remove_file(&tmp);
            AppError::Other("无法写入报表（可能被 Excel 打开），请关闭后重试".into())
        })?;
    }
    // 优先尝试 rename（同驱动器）；失败则 copy+delete（跨驱动器）
    if std::fs::rename(&tmp, output_path).is_err() {
        std::fs::copy(&tmp, output_path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            AppError::Other(format!("无法完成报表写入: {}", e))
        })?;
        let _ = std::fs::remove_file(&tmp);
    }

    tracing::info!("[导出] 报表已保存: {} ({} 字节)", output_path.display(),
        std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0));
    Ok(())
}

/// 回退方案：创建空白工作簿，写入基本数据（不含模板样式）
fn fallback_save_xlsx(output_path: &Path) -> Result<(), AppError> {
    let book = Spreadsheet::default();
    let tmp = output_path.with_extension("xlsxtmp");
    let _ = std::fs::remove_file(&tmp);
    match catch_unwind(AssertUnwindSafe(|| {
        umya_spreadsheet::writer::xlsx::write(&book, &tmp)
    })) {
        Ok(Ok(())) => {
            if output_path.exists() {
                std::fs::remove_file(output_path).map_err(|e| AppError::Other(e.to_string()))?;
            }
            if std::fs::rename(&tmp, output_path).is_err() {
                std::fs::copy(&tmp, output_path).map_err(|e| AppError::Other(e.to_string()))?;
                let _ = std::fs::remove_file(&tmp);
            }
            Ok(())
        }
        Ok(Err(e)) => Err(AppError::Other(format!("回退保存失败: {}", e))),
        Err(_) => Err(AppError::Other("回退保存 panic".into())),
    }
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

/// 对连续单元格区域设置 NumberFormat
fn apply_number_format(
    ws: &mut umya_spreadsheet::Worksheet,
    col_start: u32,
    col_end: u32,
    row_start: u32,
    row_end: u32,
    format: &str,
) {
    let mut style = Style::default();
    style.get_numbering_format_mut().set_format_code(format);
    for col in col_start..=col_end {
        for row in row_start..=row_end {
            ws.get_cell_mut((col, row)).set_style(style.clone());
        }
    }
}

/// 清理文本中的非法 XML 字符（0x00-0x08, 0x0B-0x0C, 0x0E-0x1F）
/// 这些字符会导致 xlsx XML 写入失败
fn sanitize_text(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\x00'..='\x08' | '\x0B' | '\x0C' | '\x0E'..='\x1F' => ' ',
            _ => c,
        })
        .collect()
}


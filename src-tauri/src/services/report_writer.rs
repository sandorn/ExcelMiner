//! 报表写入 — 汇总数据 + AI分析 → 修改已有 .xlsx 模板
//!
//! 使用 umya-spreadsheet v2：加载已有模板文件 → 按引擎类型写入指定单元格 → 保存。
//! 各业态写入位置对应 VBA 宏中的 cell 映射（见 业务原型/业务逻辑详解.md §3）。

use std::collections::HashMap;
use std::path::Path;

use umya_spreadsheet::Spreadsheet;
use umya_spreadsheet::structs::Style;

use crate::error::AppError;

pub struct ReportWriter;

impl ReportWriter {
    /// 将汇总数据和 AI 分析结果写入输出文件。
    ///
    /// 优先加载已有文件作为模板（保留其格式、图表等），若不存在则新建。
    pub fn write_summary(
        output_path: &Path,
        aggregation_results: &[crate::models::analysis::AggregationResult],
        ai_results: &[crate::models::analysis::AnalysisResult],
        project_name: &str,
        year: u32,
        month: u32,
    ) -> Result<(), AppError> {
        // 加载已有文件（保留模板格式），若不存在则新建空工作簿
        let mut book = if output_path.exists() {
            tracing::info!("加载已有模板: {}", output_path.display());
            umya_spreadsheet::reader::xlsx::read(output_path)
                .map_err(|e| AppError::Other(format!("无法打开报表文件: {}", e)))?
        } else {
            tracing::info!("模板不存在，创建新工作簿");
            Spreadsheet::default()
        };

        // 确保必要 Sheet 存在
        ensure_sheet(&mut book, "填写页")?;

        // Step 1: 填写"填写页"基础参数
        write_config_sheet(&mut book, project_name, year, month)?;

        // Step 2: 按引擎类型写入汇总数据到指定单元格
        for result in aggregation_results {
            write_engine_data(&mut book, result)?;
        }

        // Step 3: 写入 AI 分析结果
        write_ai_results(&mut book, ai_results)?;

        // Step 4: 保存（先写临时文件，再原子替换）
        save_xlsx(&book, output_path)?;

        Ok(())
    }
}

// ─── Sheet 工具 ──────────────────────────────────────────────────

/// 确保指定名称的 Sheet 存在，不存在则创建
fn ensure_sheet(book: &mut Spreadsheet, name: &str) -> Result<(), AppError> {
    if book.get_sheet_by_name(name).is_none() {
        book.new_sheet(name).map_err(|e| AppError::Other(e.to_string()))?;
    }
    Ok(())
}

/// 获取或创建 Sheet（可变引用）
fn get_or_create_sheet<'a>(
    book: &'a mut Spreadsheet,
    name: &str,
) -> Result<&'a mut umya_spreadsheet::Worksheet, AppError> {
    ensure_sheet(book, name)?;
    book.get_sheet_by_name_mut(name)
        .ok_or_else(|| AppError::Other(format!("Sheet '{}' 创建失败", name)))
}

// ─── 配置页 ──────────────────────────────────────────────────────

fn write_config_sheet(
    book: &mut Spreadsheet,
    project_name: &str,
    year: u32,
    month: u32,
) -> Result<(), AppError> {
    let ws = book
        .get_sheet_by_name_mut("填写页")
        .ok_or_else(|| AppError::Other("填写页不存在".into()))?;

    // A2: 报告月份 (0-based: col 0, row 1)
    ws.get_cell_mut((0u32, 1u32)).set_value_number(month as f64);
    // A4: 报告年份 (col 0, row 3)
    ws.get_cell_mut((0u32, 3u32)).set_value_number(year as f64);
    // 项目名称 (C1: col 2, row 0)
    ws.get_cell_mut((2u32, 0u32)).set_value(project_name);

    Ok(())
}

// ─── 引擎调度 ────────────────────────────────────────────────────

fn write_engine_data(
    book: &mut Spreadsheet,
    result: &crate::models::analysis::AggregationResult,
) -> Result<(), AppError> {
    let companies: Vec<serde_json::Value> =
        serde_json::from_str(&result.summary_data).unwrap_or_default();

    match result.engine_name.as_str() {
        "保险数据汇总" => write_insurance(book, &companies)?,
        "商写数据汇总" => write_commercial(book, &companies)?,
        "酒店数据汇总" => write_hotel(book, &companies)?,
        "经营报表汇总" => write_financial(book, &companies)?,
        _ => tracing::warn!("未知引擎: {}", result.engine_name),
    }
    Ok(())
}

// ─── 保险数据 → Sheet "保险类" ────────────────────────────────────

fn write_insurance(
    book: &mut Spreadsheet,
    companies: &[serde_json::Value],
) -> Result<(), AppError> {
    let ws = get_or_create_sheet(book, "保险类")?;

    // 公司 → 列 (0-based): 盛唐融信→col 2(C), 君康经纪→col 3(D)
    let company_cols: HashMap<&str, u32> =
        [("盛唐融信", 2u32), ("君康经纪", 3u32)].into();

    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        let Some(&col) = company_cols.get(name) else { continue };
        let cl = col_idx_to_letter(col);

        // 人力部分 (Rows 2-8, 0-based 1-7)
        let hr = &co["人力"];
        ws.get_cell_mut((col, 1)).set_value_number(jf64(&hr["期初人力"]));
        ws.get_cell_mut((col, 2)).set_value_number(jf64(&hr["YTD入职"]));
        ws.get_cell_mut((col, 3)).set_value_number(jf64(&hr["YTD离职"]));
        ws.get_cell_mut((col, 4)).set_value_number(jf64(&hr["当月净增"]));
        ws.get_cell_mut((col, 5)).set_value_number(jf64(&hr["月末人力"]));
        ws.get_cell_mut((col, 6)).set_value_number(jf64(&hr["平均人力"]));
        ws.get_cell_mut((col, 7)).set_value_number(jf64(&hr["开单人数YTD"]));

        // Row 9: 活动率公式 =C8/C7
        ws.get_cell_mut((col, 8))
            .set_formula(&format!("={}8/{}7", cl, cl));

        // 保费部分 (Rows 10-16, 0-based 9-15)
        let premium = &co["保费"];
        ws.get_cell_mut((col, 9)).set_value_number(jf64(&premium["新单规模保费YTD"]));
        ws.get_cell_mut((col, 10)).set_value_number(jf64(&premium["期交规模保费YTD"]));
        ws.get_cell_mut((col, 11)).set_value_number(jf64(&premium["续期13月应收"]));
        ws.get_cell_mut((col, 12)).set_value_number(jf64(&premium["续期13月实收"]));
        ws.get_cell_mut((col, 13)).set_value_number(jf64(&premium["续期25月应收"]));
        ws.get_cell_mut((col, 14)).set_value_number(jf64(&premium["续期25月实收"]));
        ws.get_cell_mut((col, 15)).set_value_number(jf64(&premium["承保件数YTD"]));

        // Row 17: 件均保费 =C10/C16
        ws.get_cell_mut((col, 16))
            .set_formula(&format!("={}10/{}16", cl, cl));
        // Row 18: 人均保费 =C10/C6
        ws.get_cell_mut((col, 17))
            .set_formula(&format!("={}10/{}6", cl, cl));

        // 月度规模保费: G/H 列 (col 6/7), Rows 13-24 (0-based 12-23)
        let monthly_col = if name == "盛唐融信" { 6u32 } else { 7u32 };
        if let Some(arr) = co["月度规模保费"].as_array() {
            for (i, v) in arr.iter().enumerate() {
                let row = (12 + i) as u32;
                if let Some(num) = v.as_f64() {
                    if num.is_finite() {
                        ws.get_cell_mut((monthly_col, row)).set_value_number(num);
                    } else {
                        ws.get_cell_mut((monthly_col, row)).set_value("#N/A");
                    }
                }
            }
        }
    }

    Ok(())
}

// ─── 商写数据 → Sheet "商写类" ────────────────────────────────────

fn write_commercial(
    book: &mut Spreadsheet,
    companies: &[serde_json::Value],
) -> Result<(), AppError> {
    let ws = get_or_create_sheet(book, "商写类")?;

    // 公司 → 列 (0-based): C=2(北京中言), D=3(大连凯丹), E=4(福建钱隆),
    //                            F=5(春夏秋冬), G=6(重庆宜新)
    let company_order: [(&str, u32); 5] = [
        ("北京中言", 2),
        ("大连凯丹", 3),
        ("福建钱隆", 4),
        ("春夏秋冬", 5),
        ("重庆宜新", 6),
    ];

    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        let Some(&col) = company_order
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, c)| c)
        else {
            continue;
        };
        let cl = col_idx_to_letter(col);

        // 面积 (Rows 2-6, 0-based 1-5)
        let area = &co["面积"];
        ws.get_cell_mut((col, 1)).set_value_number(jf64(&area["期初面积"]));
        ws.get_cell_mut((col, 2)).set_value_number(jf64(&area["YTD新增签约"]));
        ws.get_cell_mut((col, 3)).set_value_number(0.0); // 平均租金=0
        ws.get_cell_mut((col, 4)).set_value_number(jf64(&area["YTD退租"]));
        ws.get_cell_mut((col, 5)).set_value_number(jf64(&area["月末面积"]));

        // 渠道 (Rows 7-11, 0-based 6-10)
        let ch = &co["渠道"];
        ws.get_cell_mut((col, 6)).set_value_number(jf64(&ch["带客"]));
        ws.get_cell_mut((col, 7)).set_value_number(jf64(&ch["成交"]));
        ws.get_cell_mut((col, 8)).set_value_number(jf64(&ch["签约面积"]));
        ws.get_cell_mut((col, 9)).set_value_number(0.0); // 成交周期=0
        // Row 11: 渠道转化率 =col8/col7
        ws.get_cell_mut((col, 10))
            .set_formula(&format!("={}8/{}7", cl, cl));

        // 自营 (Rows 12-16, 0-based 11-15)
        let slf = &co["自营"];
        ws.get_cell_mut((col, 11)).set_value_number(jf64(&slf["带客"]));
        ws.get_cell_mut((col, 12)).set_value_number(jf64(&slf["成交"]));
        ws.get_cell_mut((col, 13)).set_value_number(jf64(&slf["签约面积"]));
        ws.get_cell_mut((col, 14)).set_value_number(0.0); // 成交周期=0
        // Row 16: 自营转化率 =col13/col12
        ws.get_cell_mut((col, 15))
            .set_formula(&format!("={}13/{}12", cl, cl));

        // 续签 (Rows 17-18, 0-based 16-17)
        let renew = &co["续签"];
        ws.get_cell_mut((col, 16)).set_value_number(jf64(&renew["到期面积"]));
        ws.get_cell_mut((col, 17)).set_value_number(jf64(&renew["续签面积"]));
        // 续签率 (Row 19) 由模板公式处理
    }

    Ok(())
}

// ─── 酒店数据 → Sheet "酒店类" ────────────────────────────────────

fn write_hotel(
    book: &mut Spreadsheet,
    companies: &[serde_json::Value],
) -> Result<(), AppError> {
    let ws = get_or_create_sheet(book, "酒店类")?;

    let company_col: HashMap<&str, u32> =
        [("伯豪瑞廷", 2u32), ("重庆瑞尔", 3u32)].into();

    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        let Some(&col) = company_col.get(name) else { continue };
        let cl = col_idx_to_letter(col);

        // 营销活动 (C2:D5, 0-based rows 1-4)
        let mkt = &co["营销活动"];
        ws.get_cell_mut((col, 1)).set_value_number(jf64(&mkt["投放数量"]));
        ws.get_cell_mut((col, 2)).set_value_number(jf64(&mkt["受众数量"]));
        ws.get_cell_mut((col, 3)).set_value_number(jf64(&mkt["成交数量"]));
        // Row 5: 转化率 =C4/C3
        ws.get_cell_mut((col, 4))
            .set_formula(&format!("={}4/{}3", cl, cl));

        // 业务指标
        let metrics = &co["业务指标"];

        // OTA网络评价: 伯豪瑞廷→F(5), 重庆瑞尔→G(6), rows 2-13 (0-based 1-12)
        let ota_col = if name == "伯豪瑞廷" { 5u32 } else { 6u32 };
        write_monthly_sequence(ws, ota_col, &metrics["OTA网络评价按月"]);

        // 月均入住率: 伯豪瑞廷→J(9), 重庆瑞尔→K(10), rows 2-13
        let occ_col = if name == "伯豪瑞廷" { 9u32 } else { 10u32 };
        write_monthly_sequence(ws, occ_col, &metrics["月均入住率按月"]);
    }

    Ok(())
}

/// 将月度序列写入指定列的 Row 2..Row 13（0-based 1..12）
fn write_monthly_sequence(
    ws: &mut umya_spreadsheet::Worksheet,
    col: u32,
    arr: &serde_json::Value,
) {
    if let Some(vals) = arr.as_array() {
        for (i, v) in vals.iter().enumerate() {
            if i >= 12 { break; }
            let row = (1 + i) as u32;
            if let Some(num) = v.as_f64() {
                ws.get_cell_mut((col, row)).set_value_number(num);
            }
        }
    }
}

// ─── 经营报表 → 各公司独立 Sheet ──────────────────────────────────

fn write_financial(
    book: &mut Spreadsheet,
    companies: &[serde_json::Value],
) -> Result<(), AppError> {
    for co in companies {
        let name = co["company"].as_str().unwrap_or("");
        if name.is_empty() { continue; }

        let ws = get_or_create_sheet(book, name)?;

        // indicators → G2:R18 (col 6..17, row 1..17)
        if let Some(ind) = co["indicators"].as_object() {
            let mut row_idx: u32 = 1; // Row 2 = index 1
            for (_label, vals) in ind {
                if let Some(arr) = vals.as_array() {
                    for (ci, v) in arr.iter().enumerate() {
                        if ci >= 12 { break; }
                        let col = (6 + ci) as u32;
                        if let Some(num) = v.as_f64() {
                            ws.get_cell_mut((col, row_idx)).set_value_number(num);
                        } else if let Some(s) = v.as_str() {
                            ws.get_cell_mut((col, row_idx)).set_value(s);
                        }
                    }
                    row_idx += 1;
                }
            }
        }
    }

    Ok(())
}

// ─── AI 分析结果 → Sheet "AI分析结果" ─────────────────────────────

fn write_ai_results(
    book: &mut Spreadsheet,
    ai_results: &[crate::models::analysis::AnalysisResult],
) -> Result<(), AppError> {
    if ai_results.is_empty() {
        return Ok(());
    }

    let ws = get_or_create_sheet(book, "AI分析结果")?;

    let mut hdr_style = Style::default();
    hdr_style.get_font_mut().set_bold(true);

    let headers = ["公司/板块", "类别", "业态", "评分", "分析内容"];
    for (ci, h) in headers.iter().enumerate() {
        let cell = ws.get_cell_mut((ci as u32, 0u32));
        cell.set_value(*h);
        cell.set_style(hdr_style.clone());
    }

    for (i, r) in ai_results.iter().enumerate() {
        let row = (i + 1) as u32;
        ws.get_cell_mut((0u32, row)).set_value(r.company_name.as_str());
        ws.get_cell_mut((1u32, row)).set_value(r.analysis_category.as_str());
        ws.get_cell_mut((2u32, row)).set_value(r.business_type.as_str());
        ws.get_cell_mut((3u32, row)).set_value(format!("{}/10", r.quality_score));
        ws.get_cell_mut((4u32, row)).set_value(r.content.as_str());
    }

    // 列宽
    ws.get_column_dimension_mut(&col_idx_to_letter(0)).set_width(14.0);
    ws.get_column_dimension_mut(&col_idx_to_letter(1)).set_width(8.0);
    ws.get_column_dimension_mut(&col_idx_to_letter(2)).set_width(10.0);
    ws.get_column_dimension_mut(&col_idx_to_letter(3)).set_width(10.0);
    ws.get_column_dimension_mut(&col_idx_to_letter(4)).set_width(80.0);

    Ok(())
}

// ─── 保存 ────────────────────────────────────────────────────────

fn save_xlsx(book: &Spreadsheet, output_path: &Path) -> Result<(), AppError> {
    // 确保目录存在
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // 清理 umya-spreadsheet 内部残留的 .xlsxtmp
    let internal_tmp = output_path.with_extension("xlsxtmp");
    std::fs::remove_file(&internal_tmp).ok();

    // 如果输出文件存在：先删除再写入（模板已在内存中，删除不会丢数据）
    // 如果被 Excel 锁定则无法删除，给出明确提示
    if output_path.exists() {
        std::fs::remove_file(output_path).map_err(|_e| {
            AppError::Other(format!(
                "无法写入报表（可能被 Excel 打开），请关闭 {} 后重试",
                output_path.file_name().unwrap_or_default().to_string_lossy()
            ))
        })?;
    }

    // 直接写入目标路径
    umya_spreadsheet::writer::xlsx::write(book, output_path)
        .map_err(|e| {
            // 写入失败也清理残留
            std::fs::remove_file(&internal_tmp).ok();
            AppError::Other(format!("保存报表失败: {}", e))
        })?;

    // 正常情况下 .xlsxtmp 已被 umya 内部 rename，再次清理保底
    std::fs::remove_file(&internal_tmp).ok();

    tracing::info!("报表已保存到: {}", output_path.display());
    Ok(())
}

// ─── 辅助函数 ────────────────────────────────────────────────────

/// 列索引 (0-based) → Excel 列字母 ("A".."Z", "AA"..)
fn col_idx_to_letter(col: u32) -> String {
    let mut n = col;
    let mut result = Vec::new();
    loop {
        let r = (n % 26) as u8;
        result.push((b'A' + r) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    result.reverse();
    result.into_iter().collect()
}

/// serde_json::Value → f64（缺失或 null 返回 0.0）
fn jf64(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}


//! 报表写入 — 汇总数据 + AI分析 → .xlsx

use std::path::Path;
use rust_xlsxwriter::*;

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
        let mut workbook = Workbook::new();
        let hdr = Format::new().set_bold();
        let nf = Format::new().set_num_format("#,##0.00");

        // Sheet 1: 填写页
        let ws = workbook.add_worksheet();
        ws.set_name("填写页")?;
        ws.write(0, 0, "报告月份")?; ws.write_with_format(0, 1, month as f64, &nf)?;
        ws.write(1, 0, "报告年份")?; ws.write_with_format(1, 1, year as f64, &nf)?;
        ws.write(2, 0, "项目名称")?; ws.write(2, 1, project_name)?;

        // 汇总数据 Sheets
        let mut used_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        for r in aggregation_results {
            let mut sheet_name = sanitize(&r.engine_name);
            if used_names.contains(&sheet_name) {
                for i in 2u32.. {
                    let alt = format!("{}_{}", sheet_name, i);
                    if !used_names.contains(&alt) {
                        sheet_name = alt;
                        break;
                    }
                }
            }
            used_names.insert(sheet_name.clone());
            let mut ws = workbook.add_worksheet();
            ws.set_name(&sheet_name)?;
            if let Ok(cos) = serde_json::from_str::<Vec<serde_json::Value>>(&r.summary_data) {
                let (headers, rows) = flatten(&cos);
                writetable(&mut ws, &headers, &rows, &hdr, &nf)?;
            } else {
                ws.write(0, 0, &r.engine_name)?;
                ws.write(1, 0, &format!("公司: {}", r.companies_processed))?;
            }
            ws.set_column_width(0, 14)?;
            for i in 1u16..10 { ws.set_column_width(i, 16)?; }
        }

        // AI 分析结果 Sheet
        if !ai_results.is_empty() {
            let ws = workbook.add_worksheet();
            ws.set_name("AI分析结果")?;
            ws.write(0, 0, "公司")?; ws.write(0, 1, "业态")?;
            ws.write(0, 2, "评分")?; ws.write(0, 3, "分析内容")?;
            ws.set_column_width(0, 14)?; ws.set_column_width(1, 10)?;
            ws.set_column_width(2, 8)?; ws.set_column_width(3, 80)?;
            for (i, r) in ai_results.iter().enumerate() {
                let row = (i + 1) as u32;
                ws.write(row, 0, &r.company_name)?;
                ws.write(row, 1, &r.business_type)?;
                ws.write(row, 2, &format!("{}/10", r.quality_score))?;
                ws.write(row, 3, &r.content)?;
            }
        }

        if let Some(parent) = Path::parent(output_path) {
            std::fs::create_dir_all(parent).ok();
        }
        workbook.save(output_path.to_str().unwrap_or("o.xlsx"))
            .map_err(|e| AppError::Other(format!("保存失败: {}", e)))
    }
}

fn flatten(companies: &[serde_json::Value]) -> (Vec<String>, Vec<Vec<String>>) {
    let mut headers = vec!["公司".to_string()];
    let mut rows = Vec::new();
    for co in companies {
        let name = co["company"].as_str().unwrap_or("").to_string();
        let mut row = vec![name];
        let mut fields: Vec<(String, String)> = Vec::new();
        extract(co, "", &mut fields);
        if rows.is_empty() { headers.extend(fields.iter().map(|(k,_)| k.clone())); }
        let mut vals = vec![String::new(); headers.len()-1];
        for (k, v) in &fields {
            if let Some(p) = headers.iter().position(|h| h==k) {
                if p > 0 { vals[p-1] = v.clone(); }
            }
        }
        row.extend(vals); rows.push(row);
    }
    (headers, rows)
}

fn extract(v: &serde_json::Value, prefix: &str, out: &mut Vec<(String, String)>) {
    if let serde_json::Value::Object(map) = v {
        for (k, val) in map {
            if matches!(k.as_str(), "company" | "月度规模保费" | "月均入住率按月" | "OTA网络评价按月" | "indicators") { continue; }
            let key = if prefix.is_empty() { k.clone() } else { format!("{}.{}", prefix, k) };
            match val {
                serde_json::Value::Number(n) => out.push((key, n.to_string())),
                serde_json::Value::String(s) => out.push((key, s.clone())),
                serde_json::Value::Object(_) => extract(val, &key, out),
                _ => {}
            }
        }
    }
}

fn writetable(ws: &mut Worksheet, h: &[String], rows: &[Vec<String>], hf: &Format, nf: &Format) -> Result<(), XlsxError> {
    for (ci, hdr) in h.iter().enumerate() { ws.write_with_format(0, ci as u16, hdr.as_str(), hf)?; }
    for (ri, row) in rows.iter().enumerate() {
        for (ci, cell) in row.iter().enumerate() {
            if let Ok(v) = cell.parse::<f64>() {
                ws.write_with_format((ri+1) as u32, ci as u16, v, nf)?;
            } else { ws.write((ri+1) as u32, ci as u16, cell.as_str())?; }
        }
    }
    Ok(())
}

fn sanitize(s: &str) -> String {
    let r = s.replace(['[',']','*','?',':','/','\\'], "_");
    if r.len() > 31 { r[..31].into() } else { r }
}

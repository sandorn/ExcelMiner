//! 酒店数据汇总引擎
//!
//! 数据源: 活动量/ + 经营报表/ 文件夹下 .xlsx
//! 汇总内容: 营销活动(投放/受众/成交) + 业务指标(OTA评分/入住率)
//! 参考 VBA: 酒店数据汇总.bas

use crate::error::AppResult;
use crate::models::analysis::{AggregationResult, PreviewData};
use crate::models::project::Project;
use crate::services::excel_reader::ExcelReader;
use crate::services::number_parser::extract_number;
use crate::services::data_aggregator::{AggregationEngine, EngineType};

// 伯豪瑞廷 营销活动行号 (多行合计)
const BHRT_PUT_ROWS: &[usize] = &[12, 13, 14];   // 投放数量: 官微/抖音/OTA
const BHRT_AUD_ROWS: &[usize] = &[15, 16, 17];   // 受众数量
const BHRT_DEAL_ROWS: &[usize] = &[18, 19, 20];  // 成交数量
// 重庆瑞尔 营销活动行号 (单行)
const CQRER_PUT_ROW: usize = 12;
const CQRER_AUD_ROW: usize = 13;
const CQRER_DEAL_ROW: usize = 14;
// 经营报表指标行号
const REPORT_ROW_OCC: usize = 15;  // 月均入住率
const REPORT_ROW_OTA: usize = 17;  // OTA网络评价

const COMPANIES: &[(&str, &str, &str, bool)] = &[
    ("伯豪瑞廷", "酒店类", "指标统计", true),   // BHRT: 达成列从E=5开始
    ("重庆瑞尔", "酒店类", "指标统计", false),   // CQRER: 达成列从D=4开始
];

pub struct HotelAggregator;

impl AggregationEngine for HotelAggregator {
    fn engine_type(&self) -> EngineType { EngineType::Hotel }
    fn name(&self) -> &str { "酒店数据汇总" }

    fn preview(&self, project: &Project) -> AppResult<PreviewData> {
        let act = project.data_folder.join("活动量");
        let rep = project.data_folder.join("经营报表");
        let mut files = Vec::new();
        for (name, _, _, _) in COMPANIES {
            if act.join(format!("{}.xlsx", name)).exists() { files.push(name.to_string()); }
            if rep.join(format!("{}.xlsx", name)).exists() { files.push(format!("{}(经营报表)", name)); }
        }
        Ok(PreviewData {
            engine_name: self.name().into(), files_found: files,
            sheets_detected: vec!["酒店类".into(), "指标统计".into()],
            companies_detected: COMPANIES.iter().map(|(n,_,_,_)| n.to_string()).collect(),
            available_indicators: vec![
                "投放数量".into(),"受众数量".into(),"成交数量".into(),
                "转化率".into(),"月均入住率".into(),"OTA网络评价".into(),
            ],
            warnings: vec![],
        })
    }

    fn execute(&self, project: &Project) -> AppResult<AggregationResult> {
        let act_folder = project.data_folder.join("活动量");
        let rep_folder = project.data_folder.join("经营报表");
        let num_months = project.ytd_months.max(1).min(12) as usize;
        let mut warnings = Vec::new();
        let mut results: Vec<serde_json::Value> = Vec::new();

        for (company_name, act_sheet, rep_sheet, is_bhrt) in COMPANIES {
            // --- 营销活动数据 ---
            let act_path = act_folder.join(format!("{}.xlsx", company_name));
            let mut reader = match ExcelReader::open(&act_path) {
                Ok(r) => r, Err(e) => {
                    warnings.push(format!("{}: 活动量打开失败 - {}", company_name, e)); continue;
                }
            };
            let data = match reader.read_sheet(act_sheet) {
                Ok(d) => d, Err(e) => {
                    warnings.push(format!("{}: 读取失败 - {}", company_name, e)); continue;
                }
            };

            let rows = &data.rows;
            let cell = |r: usize, c: usize| -> f64 {
                if r >= 1 && r-1 < rows.len() && c >= 1 && c-1 < rows[r-1].len() {
                    extract_number(&rows[r-1][c-1]).unwrap_or(0.0)
                } else { 0.0 }
            };

            // BHRT达成列从E=5开始: 2*m+3; CQRER从D=4开始: 2*m+2
            let ach_col = |m: usize| -> usize {
                if *is_bhrt { 2 * m + 3 } else { 2 * m + 2 }
            };
            let sum_rows = |rs: &[usize]| -> f64 {
                rs.iter().map(|&r| (1..=num_months).map(|m| cell(r, ach_col(m))).sum::<f64>()).sum()
            };

            let put_total: f64;
            let aud_total: f64;
            let deal_total: f64;
            if *is_bhrt {
                put_total = sum_rows(BHRT_PUT_ROWS);
                aud_total = sum_rows(BHRT_AUD_ROWS);
                deal_total = sum_rows(BHRT_DEAL_ROWS);
            } else {
                put_total = (1..=num_months).map(|m| cell(CQRER_PUT_ROW, ach_col(m))).sum();
                aud_total = (1..=num_months).map(|m| cell(CQRER_AUD_ROW, ach_col(m))).sum();
                deal_total = (1..=num_months).map(|m| cell(CQRER_DEAL_ROW, ach_col(m))).sum();
            }
            let conv_rate = if aud_total > 0.0 { deal_total / aud_total } else { 0.0 };

            // --- 业务指标(经营报表) ---
            let rep_path = rep_folder.join(format!("{}.xlsx", company_name));
            let mut occupancy_vals = Vec::new();
            let mut ota_vals = Vec::new();

            if let Ok(mut r2) = ExcelReader::open(&rep_path) {
                if let Ok(d2) = r2.read_sheet(rep_sheet) {
                    let rows2 = &d2.rows;
                    let cell2 = |r: usize, c: usize| -> f64 {
                        if r >= 1 && r-1 < rows2.len() && c >= 1 && c-1 < rows2[r-1].len() {
                            extract_number(&rows2[r-1][c-1]).unwrap_or(0.0)
                        } else { 0.0 }
                    };
                    for m in 1..=12 {
                        let col = m + 3; // D=4, E=5, ...
                        if m <= num_months {
                            occupancy_vals.push(cell2(REPORT_ROW_OCC, col));
                            ota_vals.push(cell2(REPORT_ROW_OTA, col));
                        }
                    }
                }
            }

            results.push(serde_json::json!({
                "company": company_name,
                "营销活动": { "投放数量": put_total, "受众数量": aud_total,
                    "成交数量": deal_total, "转化率": conv_rate },
                "业务指标": {
                    "月均入住率按月": occupancy_vals,
                    "OTA网络评价按月": ota_vals,
                },
            }));
        }

        Ok(AggregationResult {
            engine_name: self.name().into(),
            companies_processed: results.len(), indicators_collected: 6,
            warnings,
            summary_data: serde_json::to_string(&results).unwrap_or_default(),
        })
    }
}

//! 保险数据汇总引擎
//!
//! 数据源: 活动量/ 文件夹下 盛唐融信.xlsx 和 君康经纪.xlsx
//! 源Sheet: "保险类"
//! 参考 VBA: 保险数据汇总.bas

use std::path::PathBuf;

use crate::error::{AppError, AppResult};
use crate::models::analysis::{AggregationResult, PreviewData};
use crate::models::project::Project;
use crate::services::excel_reader::ExcelReader;
use crate::services::number_parser::extract_number;
use crate::services::data_aggregator::{AggregationEngine, EngineType};

// ---- 源行号常量 (1-based, 与VBA一致) ----
const ROW_BASE_HR: usize = 4;
const ROW_HR_IN: usize = 5;
const ROW_HR_OUT: usize = 6;
const ROW_OPEN_COUNT: usize = 10;
const ROW_NEW_PREMIUM: usize = 12;
const ROW_QJ_PREMIUM: usize = 13;
const ROW_AR_13: usize = 14;
const ROW_RC_13: usize = 15;
const ROW_AR_25: usize = 16;
const ROW_RC_25: usize = 17;
const ROW_POLICIES: usize = 18;

const COMPANIES: &[(&str, usize)] = &[
    ("盛唐融信", 3), ("君康经纪", 4),
];

pub struct InsuranceAggregator;

impl AggregationEngine for InsuranceAggregator {
    fn engine_type(&self) -> EngineType { EngineType::Insurance }
    fn name(&self) -> &str { "保险数据汇总" }

    fn preview(&self, project: &Project) -> AppResult<PreviewData> {
        let folder = project.data_folder.join("活动量");
        let mut files = Vec::new();
        for (name, _) in COMPANIES {
            let p = folder.join(format!("{}.xlsx", name));
            if p.exists() { files.push(p.to_string_lossy().to_string()); }
        }
        let file_count = files.len();
        Ok(PreviewData {
            engine_name: self.name().into(), files_found: files,
            sheets_detected: vec!["保险类".into()],
            companies_detected: COMPANIES.iter().map(|(n,_)| n.to_string()).collect(),
            available_indicators: vec![
                "期初人力".into(),"入职".into(),"离职".into(),"净增".into(),
                "月末人力".into(),"平均人力".into(),"开单人数".into(),
                "新单保费".into(),"期交保费".into(),"13月续期".into(),
                "25月续期".into(),"承保件数".into(),
            ],
            warnings: if file_count < COMPANIES.len() {
                vec!["部分公司源文件未找到".into()] } else { vec![] },
        })
    }

    fn execute(&self, project: &Project) -> AppResult<AggregationResult> {
        let folder = project.data_folder.join("活动量");
        let num_months = project.ytd_months.max(1).min(12) as usize;
        let mut warnings = Vec::new();
        let mut results: Vec<serde_json::Value> = Vec::new();

        for (company_name, _target_col) in COMPANIES {
            let path = folder.join(format!("{}.xlsx", company_name));
            let mut reader = match ExcelReader::open(&path) {
                Ok(r) => r,
                Err(e) => { warnings.push(format!("{}: 打开失败 - {}", company_name, e)); continue; }
            };
            let data = match reader.read_sheet("保险类") {
                Ok(d) => d,
                Err(e) => { warnings.push(format!("{}: 读取Sheet失败 - {}", company_name, e)); continue; }
            };

            let rows = &data.rows;
            let cell = |r: usize, c: usize| -> f64 {
                if r >= 1 && r-1 < rows.len() && c >= 1 && c-1 < rows[r-1].len() {
                    extract_number(&rows[r-1][c-1]).unwrap_or(0.0)
                } else { 0.0 }
            };
            let sum_ach = |r: usize| -> f64 {
                (1..=num_months).map(|m| cell(r, 2*m+2)).sum()
            };

            let base_hr = cell(ROW_BASE_HR, 4);
            let ytd_in = sum_ach(ROW_HR_IN);
            let ytd_out = sum_ach(ROW_HR_OUT);
            let net_add = ytd_in - ytd_out;
            let end_hr = base_hr + ytd_in - ytd_out;

            let mut cum_in = 0.0; let mut cum_out = 0.0; let mut mhr_sum = 0.0;
            for m in 1..=num_months {
                cum_in += cell(ROW_HR_IN, 2*m+2);
                cum_out += cell(ROW_HR_OUT, 2*m+2);
                mhr_sum += base_hr + cum_in - cum_out;
            }
            let avg_hr = if num_months > 0 { mhr_sum / num_months as f64 } else { 0.0 };

            let ytd_open = sum_ach(ROW_OPEN_COUNT);
            let ytd_new = sum_ach(ROW_NEW_PREMIUM);
            let ytd_qj = sum_ach(ROW_QJ_PREMIUM);
            let last_col = 2*num_months+2;
            let ar13 = cell(ROW_AR_13, last_col);
            let rc13 = cell(ROW_RC_13, last_col);
            let ar25 = cell(ROW_AR_25, last_col);
            let rc25 = cell(ROW_RC_25, last_col);
            let ytd_pol = sum_ach(ROW_POLICIES);

            let monthly: Vec<f64> = (1..=12).map(|mth| {
                if mth <= num_months { cell(ROW_NEW_PREMIUM, 2*mth+2) }
                else { f64::NAN }
            }).collect();

            let act_rate = if avg_hr > 0.0 { ytd_open / avg_hr } else { 0.0 };
            let avg_pol = if ytd_pol > 0.0 { ytd_new / ytd_pol } else { 0.0 };
            let per_cap = if avg_hr > 0.0 { ytd_new / avg_hr } else { 0.0 };

            results.push(serde_json::json!({
                "company": company_name,
                "人力": { "期初人力": base_hr, "YTD入职": ytd_in, "YTD离职": ytd_out,
                    "当月净增": net_add, "月末人力": end_hr, "平均人力": avg_hr,
                    "开单人数YTD": ytd_open },
                "保费": { "新单规模保费YTD": ytd_new, "期交规模保费YTD": ytd_qj,
                    "续期13月应收": ar13, "续期13月实收": rc13,
                    "续期25月应收": ar25, "续期25月实收": rc25,
                    "承保件数YTD": ytd_pol },
                "公式": { "活动率": act_rate, "件均保费": avg_pol, "人均保费": per_cap },
                "月度规模保费": monthly,
            }));
        }

        Ok(AggregationResult {
            engine_name: self.name().into(),
            companies_processed: results.len(),
            indicators_collected: 16,
            warnings,
            summary_data: serde_json::to_string(&results).unwrap_or_default(),
        })
    }
}

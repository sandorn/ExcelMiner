//! 商写数据汇总引擎
//!
//! 数据源: 活动量/ 文件夹下 5家公司 .xlsx
//! 源Sheet: "写字楼和商业综合体类"
//! 参考 VBA: 商写数据汇总.bas

use crate::error::AppResult;
use crate::models::analysis::{AggregationResult, PreviewData};
use crate::models::project::Project;
use crate::services::excel_reader::ExcelReader;
use crate::services::number_parser::extract_number;
use crate::services::data_aggregator::{AggregationEngine, EngineType};

// 源行号 (1-based)
const ROW_BASE_AREA: usize = 4;
const ROW_NEW_SIGN: usize = 5;
const ROW_RETREAT: usize = 7;
const ROW_END_AREA: usize = 8;
const ROW_CH_LEAD: usize = 9;
const ROW_CH_DEAL: usize = 10;
const ROW_CH_SIGN: usize = 11;
const ROW_SELF_LEAD: usize = 14;
const ROW_SELF_DEAL: usize = 15;
const ROW_SELF_SIGN: usize = 16;
const ROW_EXPIRE: usize = 19;
const ROW_RENEW: usize = 20;

const COMPANIES: &[(&str, usize)] = &[
    ("北京中言", 3), ("大连凯丹", 4), ("福建钱隆", 5),
    ("春夏秋冬", 6), ("重庆宜新", 7),
];

pub struct CommercialAggregator;

impl AggregationEngine for CommercialAggregator {
    fn engine_type(&self) -> EngineType { EngineType::Commercial }
    fn name(&self) -> &str { "商写数据汇总" }

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
            sheets_detected: vec!["写字楼和商业综合体类".into()],
            companies_detected: COMPANIES.iter().map(|(n,_)| n.to_string()).collect(),
            available_indicators: vec![
                "期初面积".into(),"新增签约面积".into(),"退租面积".into(),
                "月末面积".into(),"渠道带客".into(),"渠道成交".into(),
                "渠道签约面积".into(),"自营带客".into(),"自营成交".into(),
                "自营签约面积".into(),"到期面积".into(),"续签面积".into(),
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
                Ok(r) => r, Err(e) => {
                    warnings.push(format!("{}: 打开失败 - {}", company_name, e)); continue;
                }
            };
            let data = match reader.read_sheet("写字楼和商业综合体类") {
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
            let sum_ach = |r: usize| -> f64 {
                (1..=num_months).map(|m| cell(r, 2*m+2)).sum()
            };

            let base_area = cell(ROW_BASE_AREA, 4); // 1月达成
            let ytd_new_sign = sum_ach(ROW_NEW_SIGN);
            let ytd_retreat = sum_ach(ROW_RETREAT);
            let last = 2*num_months+2;
            let end_area = cell(ROW_END_AREA, last);
            let ytd_ch_lead = sum_ach(ROW_CH_LEAD);
            let ytd_ch_deal = sum_ach(ROW_CH_DEAL);
            let ytd_ch_sign = sum_ach(ROW_CH_SIGN);
            let ch_conv = if ytd_ch_lead > 0.0 { ytd_ch_deal / ytd_ch_lead } else { 0.0 };
            let ytd_self_lead = sum_ach(ROW_SELF_LEAD);
            let ytd_self_deal = sum_ach(ROW_SELF_DEAL);
            let ytd_self_sign = sum_ach(ROW_SELF_SIGN);
            let self_conv = if ytd_self_lead > 0.0 { ytd_self_deal / ytd_self_lead } else { 0.0 };
            let ytd_expire = sum_ach(ROW_EXPIRE);
            let ytd_renew = sum_ach(ROW_RENEW);
            let renew_rate = if ytd_expire > 0.0 { ytd_renew / ytd_expire } else { 0.0 };

            results.push(serde_json::json!({
                "company": company_name,
                "面积": { "期初面积": base_area, "YTD新增签约": ytd_new_sign,
                    "YTD退租": ytd_retreat, "月末面积": end_area },
                "渠道": { "带客": ytd_ch_lead, "成交": ytd_ch_deal,
                    "签约面积": ytd_ch_sign, "转化率": ch_conv },
                "自营": { "带客": ytd_self_lead, "成交": ytd_self_deal,
                    "签约面积": ytd_self_sign, "转化率": self_conv },
                "续签": { "到期面积": ytd_expire, "续签面积": ytd_renew,
                    "续签率": renew_rate },
            }));
        }

        Ok(AggregationResult {
            engine_name: self.name().into(),
            companies_processed: results.len(), indicators_collected: 14,
            warnings,
            summary_data: serde_json::to_string(&results).unwrap_or_default(),
        })
    }
}

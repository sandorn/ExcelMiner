//! 经营报表汇总引擎
//!
//! 数据源: 经营报表/ 文件夹下各子公司 .xlsx
//! 使用模式: 从"填写页"C2:C10读取公司列表，逐公司复制"指标统计"Sheet的D4:O20→G2:R18
//! 对应 VBA: 经营报表汇总.bas 的 ProcessSingleCompany 函数

use crate::error::AppResult;
use crate::models::analysis::{AggregationResult, PreviewData};
use crate::models::project::Project;
use crate::services::excel_reader::ExcelReader;
use crate::services::number_parser::extract_number;
use crate::services::data_aggregator::{AggregationEngine, EngineType};

pub struct FinancialAggregator;

impl AggregationEngine for FinancialAggregator {
    fn engine_type(&self) -> EngineType {
        EngineType::Financial
    }

    fn name(&self) -> &str {
        "经营报表汇总"
    }

    fn preview(&self, project: &Project) -> AppResult<PreviewData> {
        // 检查经营报表文件夹下各子公司文件
        let mut files_found = Vec::new();
        let report_folder = project.data_folder.join("经营报表");

        for company in &project.companies {
            let file_path = report_folder.join(format!("{}.xlsx", company.name));
            if file_path.exists() {
                files_found.push(file_path.to_string_lossy().to_string());
            }
        }

        let is_empty = files_found.is_empty();
        Ok(PreviewData {
            engine_name: self.name().into(),
            files_found,
            sheets_detected: vec!["指标统计".into()],
            companies_detected: project.companies.iter().map(|c| c.name.clone()).collect(),
            available_indicators: vec![
                "营业收入".into(), "营业成本".into(), "管理费用".into(),
                "销售费用".into(), "净利润".into(), "EBITDA".into(),
                "经营活动净现金流".into(), "经营支出".into(),
            ],
            warnings: if is_empty {
                vec!["经营报表文件夹为空".into()]
            } else {
                vec![]
            },
        })
    }

    fn execute(&self, project: &Project) -> AppResult<AggregationResult> {
        let folder = project.data_folder.join("经营报表");
        let mut warnings = Vec::new();
        let mut results: Vec<serde_json::Value> = Vec::new();

        // 按照 VBA: 逐公司打开经营报表→读取"指标统计"Sheet
        // 源范围 D4:O20(16行x12列) → 复制到目标 G2:R18
        for company in &project.companies {
            let path = folder.join(format!("{}.xlsx", company.name));
            let mut reader = match ExcelReader::open(&path) {
                Ok(r) => r, Err(e) => {
                    warnings.push(format!("{}: 打开失败 - {}", company.name, e)); continue;
                }
            };
            let data = match reader.read_sheet("指标统计") {
                Ok(d) => d, Err(e) => {
                    warnings.push(format!("{}: 读取失败 - {}", company.name, e)); continue;
                }
            };

            // 提取 D4:O20 区域 (0-based: rows[3..20], cols[3..15])
            let mut indicators = serde_json::Map::new();
            for (ri, row_data) in data.rows.iter().enumerate() {
                if ri < 3 || ri >= 20 { continue; } // 只取 rows 4-20 (0-based 3-19)
                let label = if ri < data.rows.len() && !row_data.is_empty() {
                    row_data[0].clone()
                } else {
                    format!("Row{}", ri + 1)
                };
                // 提取D-O列的值 (cols 3-14, 0-based)
                let vals: Vec<f64> = (3..15).map(|ci| {
                    if ci < row_data.len() {
                        extract_number(&row_data[ci]).unwrap_or(0.0)
                    } else { 0.0 }
                }).collect();
                indicators.insert(label, serde_json::Value::Array(
                    vals.iter().map(|&v| serde_json::json!(v)).collect()
                ));
            }

            results.push(serde_json::json!({
                "company": company.name,
                "sheet": "指标统计",
                "range": "D4:O20",
                "indicators": indicators,
            }));
        }

        Ok(AggregationResult {
            engine_name: self.name().into(),
            companies_processed: results.len(),
            indicators_collected: 8,
            warnings,
            summary_data: serde_json::to_string(&results).unwrap_or_default(),
        })
    }
}

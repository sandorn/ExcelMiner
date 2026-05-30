//! 仪表盘数据命令

use serde::Serialize;
use tauri::State;

use crate::commands::project_cmd::AppState;
use crate::error::AppError;

/// 仪表盘展示数据
#[derive(Debug, Clone, Serialize)]
pub struct DashboardData {
    /// KPI 卡片数据
    pub kpi_cards: Vec<KpiCard>,
    /// 月度趋势（YTD 各月各公司营业收入）
    pub monthly_trend: Vec<MonthlyPoint>,
    /// 业态营收占比
    pub segment_breakdown: Vec<SegmentShare>,
    /// 公司对比数据
    pub company_comparison: Vec<CompanyBar>,
    /// AI 分析摘要
    pub ai_summaries: Vec<AISummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KpiCard {
    pub title: String,
    pub value: f64,
    pub unit: String,
    pub trend: String, // "up" | "down" | "flat"
    pub color: String, // hex color
}

#[derive(Debug, Clone, Serialize)]
pub struct MonthlyPoint {
    pub month: String,
    pub company: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SegmentShare {
    pub segment: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompanyBar {
    pub company: String,
    pub indicator: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AISummary {
    pub company: String,
    pub summary: String,
    pub score: u32,
}

/// 获取仪表盘数据（优先使用实际汇总和分析结果，回退到演示数据）
#[tauri::command]
pub async fn get_dashboard_data(
    state: State<'_, AppState>,
) -> Result<DashboardData, AppError> {
    let agg = state.aggregation_results.lock().await;
    let ai = state.analysis_results.lock().await;

    // 如果有实际数据，尝试提取；否则返回演示数据
    if !agg.is_empty() || !ai.is_empty() {
        if let Some(data) = try_extract_real_data(&agg, &ai) {
            return Ok(data);
        }
    }

    // 演示数据（方便无实际数据时预览仪表盘效果）
    Ok(demo_data())
}

/// 尝试从实际数据中提取仪表盘内容
fn try_extract_real_data(
    agg: &[crate::models::analysis::AggregationResult],
    ai: &[crate::models::analysis::AnalysisResult],
) -> Option<DashboardData> {
    if agg.is_empty() {
        return None;
    }

    // 简单从已有数据构造 KPI（实际项目可根据 summary_data 解析具体指标）
    let total_companies: usize = agg.iter().map(|r| r.companies_processed).sum();
    let total_indicators: usize = agg.iter().map(|r| r.indicators_collected).sum();

    let summaries: Vec<AISummary> = ai
        .iter()
        .filter(|r| r.success)
        .map(|r| AISummary {
            company: r.company_name.clone(),
            summary: r.content.chars().take(120).collect(),
            score: r.quality_score,
        })
        .collect();

    Some(DashboardData {
        kpi_cards: vec![
            KpiCard {
                title: "处理公司".into(),
                value: total_companies as f64,
                unit: "家".into(),
                trend: "flat".into(),
                color: "#1890ff".into(),
            },
            KpiCard {
                title: "收集指标".into(),
                value: total_indicators as f64,
                unit: "项".into(),
                trend: "flat".into(),
                color: "#52c41a".into(),
            },
            KpiCard {
                title: "分析成功".into(),
                value: ai.iter().filter(|r| r.success).count() as f64,
                unit: "家".into(),
                trend: "up".into(),
                color: "#722ed1".into(),
            },
            KpiCard {
                title: "平均评分".into(),
                value: if ai.is_empty() {
                    0.0
                } else {
                    ai.iter().map(|r| r.quality_score as f64).sum::<f64>()
                        / ai.len() as f64
                },
                unit: "分".into(),
                trend: "up".into(),
                color: "#fa8c16".into(),
            },
        ],
        monthly_trend: vec![],
        segment_breakdown: agg
            .iter()
            .map(|r| SegmentShare {
                segment: r.engine_name.clone(),
                value: r.companies_processed as f64,
            })
            .collect(),
        company_comparison: vec![],
        ai_summaries: summaries,
    })
}

/// 演示数据（9 家公司 3 个业态的模拟经营数据）
fn demo_data() -> DashboardData {
    DashboardData {
        kpi_cards: vec![
            KpiCard { title: "营业收入".into(), value: 28450.0, unit: "万元".into(), trend: "up".into(), color: "#1890ff".into() },
            KpiCard { title: "EBITDA".into(), value: 3820.0, unit: "万元".into(), trend: "up".into(), color: "#52c41a".into() },
            KpiCard { title: "经营现金流".into(), value: 2150.0, unit: "万元".into(), trend: "down".into(), color: "#722ed1".into() },
            KpiCard { title: "综合费用率".into(), value: 62.4, unit: "%".into(), trend: "down".into(), color: "#fa8c16".into() },
        ],
        monthly_trend: vec![
            MonthlyPoint { month: "1月".into(), company: "保险A公司".into(), value: 1850.0 },
            MonthlyPoint { month: "2月".into(), company: "保险A公司".into(), value: 1920.0 },
            MonthlyPoint { month: "3月".into(), company: "保险A公司".into(), value: 2100.0 },
            MonthlyPoint { month: "4月".into(), company: "保险A公司".into(), value: 2280.0 },
            MonthlyPoint { month: "5月".into(), company: "保险A公司".into(), value: 2420.0 },
            MonthlyPoint { month: "6月".into(), company: "保险A公司".into(), value: 2600.0 },
            MonthlyPoint { month: "1月".into(), company: "酒店B公司".into(), value: 1200.0 },
            MonthlyPoint { month: "2月".into(), company: "酒店B公司".into(), value: 1350.0 },
            MonthlyPoint { month: "3月".into(), company: "酒店B公司".into(), value: 1480.0 },
            MonthlyPoint { month: "4月".into(), company: "酒店B公司".into(), value: 1420.0 },
            MonthlyPoint { month: "5月".into(), company: "酒店B公司".into(), value: 1560.0 },
            MonthlyPoint { month: "6月".into(), company: "酒店B公司".into(), value: 1700.0 },
            MonthlyPoint { month: "1月".into(), company: "商写C公司".into(), value: 980.0 },
            MonthlyPoint { month: "2月".into(), company: "商写C公司".into(), value: 1020.0 },
            MonthlyPoint { month: "3月".into(), company: "商写C公司".into(), value: 1100.0 },
            MonthlyPoint { month: "4月".into(), company: "商写C公司".into(), value: 1150.0 },
            MonthlyPoint { month: "5月".into(), company: "商写C公司".into(), value: 1080.0 },
            MonthlyPoint { month: "6月".into(), company: "商写C公司".into(), value: 1200.0 },
        ],
        segment_breakdown: vec![
            SegmentShare { segment: "保险".into(), value: 12400.0 },
            SegmentShare { segment: "酒店".into(), value: 8700.0 },
            SegmentShare { segment: "商写".into(), value: 7350.0 },
        ],
        company_comparison: vec![
            CompanyBar { company: "保险A公司".into(), indicator: "营业收入".into(), value: 8200.0 },
            CompanyBar { company: "保险B公司".into(), indicator: "营业收入".into(), value: 4200.0 },
            CompanyBar { company: "酒店A公司".into(), indicator: "营业收入".into(), value: 5100.0 },
            CompanyBar { company: "酒店B公司".into(), indicator: "营业收入".into(), value: 3600.0 },
            CompanyBar { company: "商写A公司".into(), indicator: "营业收入".into(), value: 2200.0 },
            CompanyBar { company: "商写B公司".into(), indicator: "营业收入".into(), value: 1850.0 },
            CompanyBar { company: "商写C公司".into(), indicator: "营业收入".into(), value: 1600.0 },
            CompanyBar { company: "商写D公司".into(), indicator: "营业收入".into(), value: 950.0 },
            CompanyBar { company: "商写E公司".into(), indicator: "营业收入".into(), value: 750.0 },
        ],
        ai_summaries: vec![
            AISummary { company: "保险A公司".into(), summary: "保费收入同比增长12%，13月继续率94.2%，经营稳健。新单期交保费增长显著，代理人队伍保持稳定。".into(), score: 8 },
            AISummary { company: "酒店B公司".into(), summary: "RevPAR同比提升8%，餐饮收入占比扩大至35%，能耗成本控制有效。".into(), score: 7 },
            AISummary { company: "商写C公司".into(), summary: "出租率维持在92%，但租金单价承压，需关注租户结构优化。".into(), score: 6 },
        ],
    }
}

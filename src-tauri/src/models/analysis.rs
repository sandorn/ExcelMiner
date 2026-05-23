use serde::{Deserialize, Serialize};

/// AI 分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    /// 公司/板块名称
    pub company_name: String,
    /// 业态类型
    pub business_type: String,
    /// AI 返回的分析内容
    pub content: String,
    /// 自评分 (0-10)
    pub quality_score: u32,
    /// 重试次数
    pub retry_count: u32,
    /// Token 使用量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,
    /// 是否成功
    pub success: bool,
    /// 错误信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// 分析类别：segment=板块分析, company=公司经营指标分析
    #[serde(default)]
    pub analysis_category: String,
}

/// Token 用量统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// AI 分析进度更新
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressUpdate {
    /// 当前步骤描述
    pub step: String,
    /// 进度 0.0 ~ 1.0
    pub progress: f64,
    /// 状态
    pub status: ProgressStatus,
    /// 公司名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStatus {
    Running,
    Done,
    Error,
}

/// 数据预览结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewData {
    pub engine_name: String,
    /// 发现的文件列表
    pub files_found: Vec<String>,
    /// 检测到的 Sheet
    pub sheets_detected: Vec<String>,
    /// 检测到的公司
    pub companies_detected: Vec<String>,
    /// 可用指标
    pub available_indicators: Vec<String>,
    /// 警告信息
    pub warnings: Vec<String>,
}

/// 汇总结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationResult {
    pub engine_name: String,
    /// 处理的子公司数量
    pub companies_processed: usize,
    /// 收集的指标数
    pub indicators_collected: usize,
    /// 警告
    pub warnings: Vec<String>,
    /// 汇总数据（JSON 字符串，方便前后端传输）
    pub summary_data: String,
}

/// 分析质量评估（6维度评分，满分10分）
/// 对应 VBA 核心指标分析.bas 中的 AnalysisQuality 类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisQuality {
    pub company_name: String,
    /// 是否有首行摘要（不含具体指标关键词）
    pub has_summary: bool,
    /// 是否包含营业收入指标
    pub has_revenue: bool,
    /// 是否包含 EBITDA/扣非利润指标
    pub has_ebitda: bool,
    /// 是否包含经营活动净现金流指标
    pub has_cashflow: bool,
    /// 是否包含经营支出指标
    pub has_expense: bool,
    /// 内容总行数
    pub total_lines: usize,
    /// 质量评分 0-10（每项2分）
    pub score: u32,
}

impl AnalysisQuality {
    /// 从分析内容文本计算质量评分（按业态适配维度）
    pub fn from_content(company_name: &str, content: &str, business_type: Option<&super::project::BusinessType>) -> Self {
        let lines: Vec<&str> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();
        let total_lines = lines.len();

        let content_lower = content.to_lowercase();
        let first_nonempty = lines
            .first()
            .map(|l| l.to_lowercase())
            .unwrap_or_default();

        let indicator_keywords = [
            "营业收入", "ebitda", "扣非", "经营活动",
            "经营支出", "现金流", "人力", "承保", "保费",
            "营销", "ota", "入住率", "整体情况", "合作渠道",
            "自有招商", "续租",
        ];
        let has_summary = !first_nonempty.is_empty()
            && !indicator_keywords
                .iter()
                .any(|kw| first_nonempty.contains(kw));

        // 按业态检测不同维度
        let (d1, d2, d3, d4) = match business_type {
            Some(super::project::BusinessType::Insurance) => (
                content_lower.contains("人力"),
                content_lower.contains("承保") || content_lower.contains("新单"),
                content_lower.contains("保费") || content_lower.contains("规模"),
                false,
            ),
            Some(super::project::BusinessType::Hotel) => (
                content_lower.contains("营销") || content_lower.contains("活动"),
                content_lower.contains("ota") || content_lower.contains("评价") || content_lower.contains("评分"),
                content_lower.contains("入住率") || content_lower.contains("入住"),
                false,
            ),
            Some(super::project::BusinessType::Commercial) => (
                content_lower.contains("整体") || content_lower.contains("情况"),
                content_lower.contains("合作") || content_lower.contains("渠道"),
                content_lower.contains("招商") || content_lower.contains("自有"),
                content_lower.contains("续租"),
            ),
            None => (
                content_lower.contains("营业收入"),
                content_lower.contains("ebitda") || content_lower.contains("扣非利润"),
                content_lower.contains("经营活动净现金流") || content_lower.contains("现金流"),
                content_lower.contains("经营支出"),
            ),
        };

        let mut score = 0u32;
        if has_summary { score += 2; }
        if d1 { score += 2; }
        if d2 { score += 2; }
        if d3 { score += 2; }
        if d4 { score += 2; }

        Self {
            company_name: company_name.into(),
            has_summary,
            has_revenue: d1,
            has_ebitda: d2,
            has_cashflow: d3,
            has_expense: d4,
            total_lines,
            score,
        }
    }
}

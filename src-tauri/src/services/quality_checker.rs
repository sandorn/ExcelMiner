//! 分析质量检查（5维度评分阈值校验，与 AardMiner/VBA 一致）
//!
//! 评分维度：摘要/营业收入/EBITDA(GOP/扣非)/现金流/支出 各2分，满分10分

use serde::{Deserialize, Serialize};

use crate::models::analysis::AnalysisQuality;
use crate::models::project::BusinessType;

/// 质量检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityResult {
    pub score: u32,
    pub passed: bool,
    pub reason: Option<String>,
    /// 详细维度得分
    pub details: AnalysisQuality,
}

/// 质量检查器
pub struct QualityChecker {
    /// 通过阈值 (0-10)
    threshold: u32,
    /// 最大重试次数（对应 VBA 的 MAX_RETRIES=2）
    max_retries: u32,
}

impl QualityChecker {
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold: threshold.max(1).min(10),
            max_retries: 2,
        }
    }

    /// 从分析内容计算质量评分（按业态适配评分维度）
    pub fn evaluate(&self, company_name: &str, content: &str, business_type: Option<&BusinessType>) -> QualityResult {
        let details = AnalysisQuality::from_content(company_name, content, business_type);
        let score = details.score;

        QualityResult {
            score,
            passed: score >= self.threshold,
            reason: if score < self.threshold {
                Some(format!(
                    "评分 {} 低于阈值 {} (营收:{}, EBITDA/GOP/扣非:{}, 现金流:{}, 支出:{})",
                    score,
                    self.threshold,
                    if details.has_revenue { "✓" } else { "✗" },
                    if details.has_ebitda { "✓" } else { "✗" },
                    if details.has_cashflow { "✓" } else { "✗" },
                    if details.has_expense { "✓" } else { "✗" },
                ))
            } else {
                None
            },
            details,
        }
    }

    /// 检查内容是否有效（不为空且长度达标）
    pub fn is_valid_content(&self, content: &str) -> bool {
        let trimmed = content.trim();
        !trimmed.is_empty() && trimmed.len() >= 50
    }

    /// 获取允许的最大重试次数
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// 生成质量不足提示文本（对应 VBA 的 QualityTooLow 消息）
    pub fn quality_hint(&self, quality: &AnalysisQuality) -> String {
        format!(
            "[质量提示：本分析质量评分 {}/10，部分指标描述可能不完整]",
            quality.score
        )
    }
}

impl Default for QualityChecker {
    fn default() -> Self {
        Self {
            threshold: 8,
            max_retries: 2,
        }
    }
}

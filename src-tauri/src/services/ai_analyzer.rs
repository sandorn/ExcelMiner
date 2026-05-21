//! DeepSeek API 调用封装

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::models::analysis::{AnalysisResult, ProgressUpdate, TokenUsage};
use crate::models::project::{AIConfig, BusinessType};

/// DeepSeek API 消息
#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// DeepSeek API 请求体
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
    max_tokens: u32,
}

/// DeepSeek API 响应
#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    usage: Option<UsageInfo>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: String,
}

#[derive(Debug, Deserialize)]
struct UsageInfo {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// AI 分析器
pub struct AIAnalyzer {
    config: AIConfig,
    client: reqwest::Client,
}

impl AIAnalyzer {
    pub fn new(config: AIConfig) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| AppError::Other(e.to_string()))?;

        Ok(Self { config, client })
    }

    /// 加载系统提示词
    pub fn load_system_prompt(&self) -> AppResult<String> {
        if self.config.system_prompt_path.as_os_str().is_empty() {
            return Ok(DEFAULT_SYSTEM_PROMPT.to_string());
        }
        let content = std::fs::read_to_string(&self.config.system_prompt_path)
            .map_err(|e| AppError::Io(format!(
                "无法读取提示词文件 {:?}: {}",
                self.config.system_prompt_path, e
            )))?;
        Ok(content)
    }

    /// 单次 API 调用
    pub async fn call(&self, system_prompt: &str, user_prompt: &str) -> AppResult<(String, Option<TokenUsage>)> {
        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: system_prompt.into(),
                },
                ChatMessage {
                    role: "user".into(),
                    content: user_prompt.into(),
                },
            ],
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        };

        let response = self
            .client
            .post(&self.config.api_url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::ApiError {
                retry: 0,
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::ApiError {
                retry: 0,
                message: format!("HTTP {}: {}", status, body),
            });
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| AppError::ApiError {
                retry: 0,
                message: format!("JSON 解析失败: {}", e),
            })?;

        let content = chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        let usage = chat_response.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        Ok((content, usage))
    }

    /// 分批分析（带重试和质量检查）
    pub async fn analyze_batch<F>(
        &self,
        business_type: BusinessType,
        companies_data: &[(String, String)],  // (公司名, 汇总数据文本)
        on_progress: F,
    ) -> AppResult<Vec<AnalysisResult>>
    where
        F: Fn(ProgressUpdate) + Send + Sync,
    {
        let system_prompt = self.load_system_prompt()?;
        let total_companies = companies_data.len();
        let batch_size = self.config.batch_size.max(1);
        let mut results = Vec::new();

        let batches: Vec<_> = companies_data.chunks(batch_size).collect();
        let total_batches = batches.len();

        for (batch_idx, batch) in batches.iter().enumerate() {
            for (company_name, data_text) in *batch {
                on_progress(ProgressUpdate {
                    step: format!(
                        "正在分析 {}业态 → {} (第{}/{}批)",
                        business_type,
                        company_name,
                        batch_idx + 1,
                        total_batches
                    ),
                    progress: (batch_idx as f64) / (total_batches as f64),
                    status: crate::models::analysis::ProgressStatus::Running,
                    company: Some(company_name.clone()),
                });

                let user_prompt = format!(
                    "请分析以下{}业态子公司 [{}] 的经营数据：\n\n{}",
                    business_type, company_name, data_text
                );

                // 带重试的调用
                let mut final_result = None;
                for retry in 0..self.config.max_retries {
                    match self.call(&system_prompt, &user_prompt).await {
                        Ok((content, usage)) => {
                            // 质量检查 — 6维度评分（无需额外API调用）
                            use crate::services::quality_checker::QualityChecker;
                            let checker = QualityChecker::new(self.config.quality_threshold);
                            let qr = checker.evaluate(company_name, &content);
                            let score = qr.score;

                            if score >= self.config.quality_threshold {
                                final_result = Some(AnalysisResult {
                                    company_name: company_name.clone(),
                                    business_type: business_type.to_string(),
                                    content,
                                    quality_score: score,
                                    retry_count: retry,
                                    token_usage: usage,
                                    success: true,
                                    error_message: None,
                                });
                                break;
                            } else {
                                tracing::warn!(
                                    "质量评分不足: {} (得分 {}/{})，重试 {}/{}",
                                    company_name, score, self.config.quality_threshold,
                                    retry + 1, self.config.max_retries
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!("API 调用失败 ({}/{}): {}", retry + 1, self.config.max_retries, e);
                            if retry == self.config.max_retries - 1 {
                                final_result = Some(AnalysisResult {
                                    company_name: company_name.clone(),
                                    business_type: business_type.to_string(),
                                    content: String::new(),
                                    quality_score: 0,
                                    retry_count: retry + 1,
                                    token_usage: None,
                                    success: false,
                                    error_message: Some(e.to_string()),
                                });
                            }
                        }
                    }
                }

                if let Some(result) = final_result {
                    results.push(result);
                }
            }
        }

        on_progress(ProgressUpdate {
            step: format!("{}业态分析完成", business_type),
            progress: 1.0,
            status: crate::models::analysis::ProgressStatus::Done,
            company: None,
        });

        Ok(results)
    }

    /// 加载系统提示词（内部方法）
    fn _load_prompt(path: &std::path::Path) -> AppResult<String> {
        Ok(std::fs::read_to_string(path)
            .unwrap_or_else(|_| DEFAULT_SYSTEM_PROMPT.to_string()))
    }
}

pub const DEFAULT_SYSTEM_PROMPT: &str = r#"你是一位专业的财务分析师，擅长分析各业态子公司的经营数据。
请根据提供的数据进行深入分析，包括但不限于：
1. 与上月、去年同期的对比分析
2. 各项指标的达成情况
3. 存在的问题和风险提示
4. 改进建议
请以PPT半页文案的形式输出，语言精炼，要点突出。"#;

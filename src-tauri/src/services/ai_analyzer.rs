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

    /// 加载系统提示词（支持按业态自动匹配）
    pub fn load_system_prompt(&self, business_type: Option<&BusinessType>) -> AppResult<String> {
        // 1. 如果指定了路径，从文件加载
        if !self.config.system_prompt_path.as_os_str().is_empty() {
            return Ok(std::fs::read_to_string(&self.config.system_prompt_path)
                .unwrap_or_else(|_| default_prompt_for(business_type)));
        }
        // 2. 按业态返回默认提示词
        Ok(default_prompt_for(business_type))
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
        let system_prompt = self.load_system_prompt(Some(&business_type))?;
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
            .unwrap_or_else(|_| PROMPT_FINANCIAL.to_string()))
    }
}

pub const PROMPT_INSURANCE: &str = r#"# 角色
严谨的保险中介经营数据分析师。

# 任务
基于提供的【两家保险中介子公司】经营数据（表格形式），输出一段客观、精简、可直接粘贴到PPT半页的分析。

## 核心规则
1. **摘要**（1-2句，≤40字）：概括两家公司最突出的业务特征差异。
2. **分段描述**：必须按以下三个小标题顺序输出（每条≤50字）：
   - `人力与活动：`
   - `承保新单及效率：`
   - `月度规模保费：`
3. **内容要求**：只陈述数值差异，不做因果推断。禁止分析原因、提建议、使用主观情绪词。
4. **缺失值处理**：#N/A或空白视为无数据，分析时跳过该月份，不提及。
5. **输出格式**：第一行直接写摘要（无标签），然后每段以小标题开头，冒号后接描述，各段之间不空行。"#;

pub const PROMPT_COMMERCIAL: &str = r#"# 角色
严谨的商写租赁经营分析师。

# 任务
基于提供的【商写租赁类子公司】经营数据，输出一段客观、精简、可直接粘贴到PPT半页的分析。期初面积和月末面积不作为经营成果分析。

## 核心规则
1. **首条**（1-2句，≤40字）：概括所有公司中最突出的经营特征。不得提及期初面积、月末面积。
2. **分段描述**：必须按以下四个小标题顺序输出（每条≤50字）：
   - `整体情况：`
   - `合作渠道：`
   - `自有招商：`
   - `续租情况：`
3. **内容要求**：只做横向对比和客观列举。自动剔除全0指标。禁止分析原因、提建议。
4. **输出格式**：第一行直接写摘要（无标签），然后每段以小标题开头，冒号后接描述，各段之间不空行。"#;

pub const PROMPT_HOTEL: &str = r#"# 角色
严谨的酒店经营数据分析师。

# 任务
基于提供的【两家酒店子公司】经营数据，输出一段客观、精简、可直接粘贴到PPT半页的分析。

## 核心规则
1. **摘要**（1-2句，≤40字）：概括两家酒店最突出的经营特征差异。
2. **分段描述**：必须按以下三个小标题顺序输出（每条≤50字）：
   - `营销活动：`
   - `OTA评分：`
   - `月均入住率：`
3. **内容要求**：只陈述数值差异，不做因果推断。禁止分析原因、提建议、使用主观情绪词。
4. **缺失值处理**：#N/A或空白视为无数据，分析时跳过该月份，不提及。
5. **输出格式**：第一行直接写摘要（无标签），然后每段以小标题开头，冒号后接描述，各段之间不空行。"#;

pub const PROMPT_FINANCIAL: &str = r#"## 角色
严谨的高级财务分析师。

## 任务
基于给定的公司经营数据（单位：万元），生成一段精简、客观、可直接粘贴到PPT半页的分析。

## 核心规则
1. **序时进度**：以用户消息中提供的值为准，禁止自行计算或改动。
2. **达成率对比**：达成率>序时进度→"领先X.X个百分点"，<→"落后X.X个百分点"。
3. **环比趋势**：收入/现金流增→"环比上升"，利润类亏损收窄→"环比减亏"。
4. **利润类目标为负**：按绝对值表述。
5. **支出/成本类**：列示累计与达成率，内部判断"成本管控有效"或"刚性成本较高"。
6. **波动性**：(max-min)/均值>30%→"波动大"。
7. **绝对禁止**：主观情绪词、因果推断、改进建议。

## 输出格式
- 首行摘要≤50字，以"[年份]年前[月份]个月，[公司名称]"开头。
- 后续每行一个指标（≤60字），包含累计数、达成率、环比趋势、波动性。"#;

fn default_prompt_for(business_type: Option<&BusinessType>) -> String {
    match business_type {
        Some(BusinessType::Insurance) => PROMPT_INSURANCE.to_string(),
        Some(BusinessType::Commercial) => PROMPT_COMMERCIAL.to_string(),
        Some(BusinessType::Hotel) => PROMPT_HOTEL.to_string(),
        _ => PROMPT_FINANCIAL.to_string(),
    }
}

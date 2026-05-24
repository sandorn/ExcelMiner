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
        // 2. 尝试从可执行目录 ../resources/prompts/ 加载（便携版兼容）
        if let Some(bt) = business_type {
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    let fname = match bt {
                        BusinessType::Insurance => "保险分析.md",
                        BusinessType::Commercial => "商写分析.md",
                        BusinessType::Hotel => "酒店分析.md",
                    };
                    let path = dir.join("resources").join("prompts").join(fname);
                    if path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            return Ok(content);
                        }
                    }
                }
            }
        }
        // 3. 按业态返回内置默认提示词
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

    /// 板块级分析（跳过质量检查，仅重试 API 调用）
    /// 用于业态板块汇总分析，不对输出做维度评分
    pub async fn analyze_segment(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        company_name: &str,
        business_type_display: &str,
    ) -> AnalysisResult {
        tracing::info!(
            "[板块分析] {}: system={}chars user={}chars",
            company_name,
            system_prompt.len(),
            user_prompt.len()
        );

        let mut last_content = String::new();
        let mut last_usage: Option<TokenUsage> = None;

        for retry in 0..self.config.max_retries {
            match self.call(system_prompt, user_prompt).await {
                Ok((content, usage)) => {
                    let trimmed = content.trim();
                    if trimmed.len() >= 50 {
                        tracing::info!("[板块分析] {} 完成(第{}次): {}字", company_name, retry + 1, trimmed.len());
                        return AnalysisResult {
                            company_name: company_name.to_string(),
                            business_type: business_type_display.to_string(),
                            content: trimmed.to_string(),
                            quality_score: 0,
                            retry_count: retry,
                            token_usage: usage,
                            success: true,
                            error_message: None,
                            analysis_category: "segment".to_string(),
                        };
                    }
                    tracing::warn!(
                        "[板块分析] {} 内容过短({}字) 重试{}/{}",
                        company_name, trimmed.len(), retry + 1, self.config.max_retries
                    );
                    last_content = trimmed.to_string();
                    last_usage = usage;
                }
                Err(e) => {
                    tracing::error!(
                        "[板块分析] {} API失败({}) 重试{}/{}",
                        company_name, e, retry + 1, self.config.max_retries
                    );
                    if retry == self.config.max_retries - 1 {
                        let has_content = !last_content.is_empty();
                        return AnalysisResult {
                            company_name: company_name.to_string(),
                            business_type: business_type_display.to_string(),
                            content: last_content.clone(),
                            quality_score: 0,
                            retry_count: retry + 1,
                            token_usage: last_usage,
                            success: has_content,
                            error_message: if has_content {
                                None
                            } else {
                                Some(e.to_string())
                            },
                            analysis_category: "segment".to_string(),
                        };
                    }
                }
            }
        }

        // 重试耗尽
        let has_content = !last_content.is_empty();
        AnalysisResult {
            company_name: company_name.to_string(),
            business_type: business_type_display.to_string(),
            content: last_content.clone(),
            quality_score: 0,
            retry_count: self.config.max_retries,
            token_usage: last_usage,
            success: has_content,
            error_message: if has_content {
                None
            } else {
                Some("[板块分析] 内容为空，重试已耗尽".to_string())
            },
            analysis_category: "segment".to_string(),
        }
    }

    /// 分批分析（带重试和质量检查）
    /// 每批将多个公司的数据合并到一次 API 调用中，由 batch_size 控制每批公司数
    pub async fn analyze_batch<F>(
        &self,
        business_type: BusinessType,
        companies_data: &[(String, String)],
        custom_prompt: Option<&str>,
        on_progress: F,
    ) -> AppResult<Vec<AnalysisResult>>
    where
        F: Fn(ProgressUpdate) + Send + Sync,
    {
        let system_prompt = if let Some(p) = custom_prompt {
            if !p.trim().is_empty() { p.to_string() } else {
                self.load_system_prompt(Some(&business_type))?
            }
        } else {
            self.load_system_prompt(Some(&business_type))?
        };

        let batch_size = self.config.batch_size.max(1);
        let batches: Vec<_> = companies_data.chunks(batch_size).collect();
        let total_batches = batches.len();
        let mut results = Vec::new();

        for (batch_idx, batch) in batches.iter().enumerate() {
            let company_names: Vec<_> = batch.iter().map(|(n, _)| n.as_str()).collect();
            let names_str = company_names.join("、");

            on_progress(ProgressUpdate {
                step: format!(
                    "正在分析 {}业态 → {} (第{}/{}批)",
                    business_type, names_str, batch_idx + 1, total_batches
                ),
                progress: (batch_idx as f64) / (total_batches as f64),
                status: crate::models::analysis::ProgressStatus::Running,
                company: Some(names_str.clone()),
            });

            // 将同批公司的数据合并到一个 prompt
            let combined_data: Vec<String> = batch
                .iter()
                .map(|(name, data)| format!("【{}】\n{}", name, data))
                .collect();
            let user_prompt = format!(
                "请分析以下{}业态子公司的经营数据：\n\n{}",
                business_type,
                combined_data.join("\n\n---\n\n")
            );

            // 带重试的调用
            let mut batch_results: Vec<AnalysisResult> = Vec::new();
            for retry in 0..self.config.max_retries {
                match self.call(&system_prompt, &user_prompt).await {
                    Ok((content, usage)) => {
                        let checker = crate::services::quality_checker::QualityChecker::new(
                            self.config.quality_threshold,
                        );

                        // 反解：将结果分配给各公司
                        for (company_name, _data_text) in *batch {
                            // 尝试从分析结果中提取该公司相关内容
                            let company_content = extract_company_section(&content, company_name)
                                .unwrap_or_else(|| content.clone());

                            let qr = checker.evaluate(company_name, &company_content, Some(&business_type));
                            let score = qr.score;

                            if score >= self.config.quality_threshold {
                                batch_results.push(AnalysisResult {
                                    company_name: company_name.to_string(),
                                    business_type: business_type.to_string(),
                                    content: company_content,
                                    quality_score: score,
                                    retry_count: retry,
                                    token_usage: usage.clone(),
                                    success: true,
                                    error_message: None,
                                    analysis_category: String::new(),
                                });
                            } else {
                                tracing::warn!(
                                    "质量评分不足: {} (得分 {}/{})，整批重试 {}/{}",
                                    company_name, score, self.config.quality_threshold,
                                    retry + 1, self.config.max_retries
                                );
                                // 重试整体batch
                                batch_results.clear();
                                break;
                            }
                        }

                        if !batch_results.is_empty() {
                            break; // 整批通过
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "API 调用失败 ({}/{}): {}",
                            retry + 1, self.config.max_retries, e
                        );
                        if retry == self.config.max_retries - 1 {
                            for (company_name, _data_text) in *batch {
                                batch_results.push(AnalysisResult {
                                    company_name: company_name.to_string(),
                                    business_type: business_type.to_string(),
                                    content: String::new(),
                                    quality_score: 0,
                                    retry_count: retry + 1,
                                    token_usage: None,
                                    success: false,
                                    error_message: Some(e.to_string()),
                                    analysis_category: String::new(),
                                });
                            }
                        }
                    }
                }
            }
            // 所有重试耗尽仍不通过 → 保留最后一次的结果（即使评分不足）
            if batch_results.is_empty() {
                for (company_name, _data_text) in *batch {
                    batch_results.push(AnalysisResult {
                        company_name: company_name.to_string(),
                        business_type: business_type.to_string(),
                        content: String::new(),
                        quality_score: 0,
                        retry_count: self.config.max_retries,
                        token_usage: None,
                        success: false,
                        error_message: Some(format!(
                            "质量评分均低于阈值 {}，已重试 {} 次",
                            self.config.quality_threshold, self.config.max_retries
                        )),
                        analysis_category: String::new(),
                    });
                }
            }
            results.extend(batch_results);
        }

        on_progress(ProgressUpdate {
            step: format!("{}业态分析完成", business_type),
            progress: 1.0,
            status: crate::models::analysis::ProgressStatus::Done,
            company: None,
        });

        Ok(results)
    }

    /// 单次分析（带重试和质量检查），返回单个 AnalysisResult
    /// 重试耗尽时保留最后一次内容并标注【质量不达标】
    pub async fn analyze_single(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        company_name: &str,
        business_type_display: &str,
        business_type_enum: Option<&BusinessType>,
        analysis_category: &str,
    ) -> AnalysisResult {
        let checker = crate::services::quality_checker::QualityChecker::new(
            self.config.quality_threshold,
        );

        // 记录经营分析输入数据
        tracing::info!(
            "[经营分析] {}: system={}chars user={}chars\n---user_prompt---\n{}\n---end---",
            company_name,
            system_prompt.len(),
            user_prompt.len(),
            user_prompt
        );

        let mut last_content = String::new();
        let mut last_score = 0u32;
        let mut last_usage: Option<TokenUsage> = None;

        for retry in 0..self.config.max_retries {
            match self.call(system_prompt, user_prompt).await {
                Ok((content, usage)) => {
                    let content_len = content.len();
                    let qr = checker.evaluate(company_name, &content, business_type_enum);
                    let score = qr.score;
                    last_content = content;
                    last_score = score;
                    last_usage = usage.clone();

                    tracing::info!(
                        "[经营分析] {} (第{}次): 得分 {}/{} 内容{}字 摘要={} 营收={} ebitda={} 现金流={} 支出={}",
                        company_name,
                        retry + 1,
                        score,
                        self.config.quality_threshold,
                        content_len,
                        if qr.details.has_summary { "✓" } else { "✗" },
                        if qr.details.has_revenue { "✓" } else { "✗" },
                        if qr.details.has_ebitda { "✓" } else { "✗" },
                        if qr.details.has_cashflow { "✓" } else { "✗" },
                        if qr.details.has_expense { "✓" } else { "✗" },
                    );

                    if score >= self.config.quality_threshold {
                        return AnalysisResult {
                            company_name: company_name.to_string(),
                            business_type: business_type_display.to_string(),
                            content: last_content,
                            quality_score: score,
                            retry_count: retry,
                            token_usage: last_usage,
                            success: true,
                            error_message: None,
                            analysis_category: analysis_category.to_string(),
                        };
                    }

                    tracing::warn!(
                        "质量评分不足: {} (得分 {}/{})，重试 {}/{}",
                        company_name, score, self.config.quality_threshold,
                        retry + 1, self.config.max_retries
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "[经营分析] {} API失败({}) 重试{}/{}",
                        company_name, e,
                        retry + 1, self.config.max_retries
                    );
                    if retry == self.config.max_retries - 1 {
                        return AnalysisResult {
                            company_name: company_name.to_string(),
                            business_type: business_type_display.to_string(),
                            content: String::new(),
                            quality_score: 0,
                            retry_count: retry + 1,
                            token_usage: None,
                            success: false,
                            error_message: Some(e.to_string()),
                            analysis_category: analysis_category.to_string(),
                        };
                    }
                }
            }
        }

        // 所有重试耗尽 — 保留最后一次内容，标记质量不达标
        if !last_content.is_empty() {
            tracing::warn!(
                "{} 重试耗尽，保留最后内容 (得分 {}/{})，标注质量不达标",
                company_name, last_score, self.config.quality_threshold
            );
            let marked = format!(
                "【质量不达标，评分 {}/{}】\n\n{}",
                last_score, self.config.quality_threshold, last_content
            );
            AnalysisResult {
                company_name: company_name.to_string(),
                business_type: business_type_display.to_string(),
                content: marked,
                quality_score: last_score,
                retry_count: self.config.max_retries,
                token_usage: last_usage,
                success: true,
                error_message: None,
                analysis_category: analysis_category.to_string(),
            }
        } else {
            AnalysisResult {
                company_name: company_name.to_string(),
                business_type: business_type_display.to_string(),
                content: String::new(),
                quality_score: 0,
                retry_count: self.config.max_retries,
                token_usage: None,
                success: false,
                error_message: Some(format!(
                    "质量评分均低于阈值 {}，已重试 {} 次",
                    self.config.quality_threshold, self.config.max_retries
                )),
                analysis_category: analysis_category.to_string(),
            }
        }
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

/// 从整批分析结果中提取单个公司的段落
fn extract_company_section(content: &str, company_name: &str) -> Option<String> {
    // 尝试匹配 【公司名】 或 公司名： 或 "公司名" 开头的段落
    let lines: Vec<&str> = content.lines().collect();
    let mut start_idx = None;
    let patterns = [
        format!("【{}】", company_name),
        format!("{}：", company_name),
        format!("{}:", company_name),
    ];

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if patterns.iter().any(|p| trimmed.contains(p)) {
            start_idx = Some(i);
            break;
        }
    }

    let start = start_idx?;
    // 到下一个公司标记或文末
    let end = lines[start + 1..]
        .iter()
        .position(|l| {
            let t = l.trim();
            t.starts_with('【') || t.contains("：") && !t.starts_with('-')
        })
        .map(|p| start + 1 + p)
        .unwrap_or(lines.len());

    Some(lines[start..end].join("\n"))
}

fn default_prompt_for(business_type: Option<&BusinessType>) -> String {
    match business_type {
        Some(BusinessType::Insurance) => PROMPT_INSURANCE.to_string(),
        Some(BusinessType::Commercial) => PROMPT_COMMERCIAL.to_string(),
        Some(BusinessType::Hotel) => PROMPT_HOTEL.to_string(),
        _ => PROMPT_FINANCIAL.to_string(),
    }
}

//! DeepSeek API 调用封装

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::models::analysis::{AnalysisResult, ProgressUpdate, TokenUsage};
use crate::models::project::{AIConfig, BusinessType};
use crate::utils::log_sanitizer::sanitize_key;

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
    /// 创建 AI 分析器，HTTP 超时从 AppConfig.tuning.api_timeout_secs 读取（默认 60s）
    pub fn new(config: AIConfig) -> AppResult<Self> {
        Self::with_timeout(config, 60)
    }

    /// 创建 AI 分析器并指定超时秒数
    pub fn with_timeout(config: AIConfig, timeout_secs: u64) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| AppError::Other(e.to_string()))?;

        Ok(Self { config, client })
    }

    /// 加载系统提示词（始终从本地配置文件加载）
    ///
    /// 查找优先级：
    /// 1. 用户指定的 system_prompt_path
    /// 2. 可执行文件 ../resources/prompts/ （便携版路径）
    /// 3. 工作目录 resources/prompts/ （开发模式路径）
    /// 4. 最小兜底提示词（仅用于确保程序不崩溃）
    pub fn load_system_prompt(&self, business_type: Option<&BusinessType>) -> AppResult<String> {
        let fname = match business_type {
            Some(BusinessType::Insurance) => "保险分析.md",
            Some(BusinessType::Commercial) => "商写分析.md",
            Some(BusinessType::Hotel) => "酒店分析.md",
            None => "经营分析.md",
        };

        // 1. 用户指定路径
        if !self.config.system_prompt_path.as_os_str().is_empty() {
            if let Ok(content) = std::fs::read_to_string(&self.config.system_prompt_path) {
                return Ok(content);
            }
            tracing::warn!("用户指定提示词文件不存在: {:?}", self.config.system_prompt_path);
        }

        // 2. 可执行文件 ../resources/prompts/ （便携版路径）
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let path = dir.join("resources").join("prompts").join(fname);
                if path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        return Ok(content);
                    }
                }
            }
        }

        // 3. 工作目录 resources/prompts/ （开发模式路径）
        let dev_path = std::path::Path::new("resources").join("prompts").join(fname);
        if dev_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&dev_path) {
                return Ok(content);
            }
        }

        // 4. 最小兜底提示词
        tracing::error!("无法加载提示词文件 '{}'，使用最小兜底提示词", fname);
        Ok(FALLBACK_PROMPT.to_string())
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

        let api_url = &self.config.api_url;
        let api_key_preview = sanitize_key(&self.config.api_key);
        tracing::info!("[API] → POST {} | key={} | model={} | temp={} | max_tok={} | sys={}ch user={}ch",
            api_url, api_key_preview, self.config.model, self.config.temperature,
            self.config.max_tokens, system_prompt.len(), user_prompt.len());

        let start = std::time::Instant::now();
        let response = self
            .client
            .post(api_url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("[API] 网络错误({:.1}ms): {}", start.elapsed().as_secs_f64() * 1000.0, e);
                AppError::ApiError {
                    retry: 0,
                    message: e.to_string(),
                }
            })?;

        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let status = response.status();
        tracing::info!("[API] ← HTTP {} ({:.0}ms)", status, elapsed_ms);

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let body_preview = if body.len() > 200 { &body[..200] } else { &body };
            tracing::error!("[API] HTTP错误 {} body: {}", status, body_preview);
            return Err(AppError::ApiError {
                retry: 0,
                message: format!("HTTP {}: {}", status, body),
            });
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| {
                tracing::error!("[API] JSON解析失败({:.0}ms): {}", elapsed_ms, e);
                AppError::ApiError {
                    retry: 0,
                    message: format!("JSON 解析失败: {}", e),
                }
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

        let content_preview = if content.len() > 120 {
            let end = content.char_indices().nth(120).map(|(i,_)| i).unwrap_or(content.len());
            format!("{}...", &content[..end])
        } else { content.clone() };
        tracing::info!("[API] 响应: {}字 | tokens={:?} | preview={}",
            content.len(), usage.as_ref().map(|u| (u.prompt_tokens, u.completion_tokens, u.total_tokens)),
            content_preview);

        Ok((content, usage))
    }

    /// 板块级分析（跳过质量检查，仅检查内容长度≥50字）
    /// 用于业态板块汇总分析，不对输出做维度评分
    pub async fn analyze_segment(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        company_name: &str,
        business_type_display: &str,
    ) -> AnalysisResult {
        tracing::info!(
            "[板块分析] {} START | sys={}chars user={}chars retries={}",
            company_name, system_prompt.len(), user_prompt.len(), self.config.max_retries
        );
        tracing::debug!(
            "[板块分析] {} user_prompt:\n{}\n---end---",
            company_name, user_prompt
        );

        let mut last_content = String::new();
        let mut last_usage: Option<TokenUsage> = None;

        for retry in 0..self.config.max_retries {
            // 指数退避: 2^retry × 1000ms (首次0ms, 然后1s, 2s, 4s...)
            if retry > 0 {
                let delay_ms = (1u64 << (retry - 1)) * 1000;
                tracing::info!("[板块分析] {} 重试{}/{} 等待{}ms", company_name, retry + 1, self.config.max_retries, delay_ms);
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
            tracing::info!("[板块分析] {} 第{}次调用 API...", company_name, retry + 1);
            match self.call(system_prompt, user_prompt).await {
                Ok((content, usage)) => {
                    let trimmed = content.trim();
                    if trimmed.len() >= 50 {
                        tracing::info!("[板块分析] {} ✅ 成功(第{}次): {}字", company_name, retry + 1, trimmed.len());
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
                        "[板块分析] {} ⚠ 内容过短({}字, 需≥50) 重试{}/{} | 内容: {}",
                        company_name, trimmed.len(), retry + 1, self.config.max_retries,
                        if trimmed.is_empty() { "(空)" } else { trimmed }
                    );
                    tracing::info!(
                        "[板块分析] {} 内容不足50字，正在重试 ({}/{})...",
                        company_name, retry + 1, self.config.max_retries
                    );
                    last_content = trimmed.to_string();
                    last_usage = usage;
                }
                Err(e) => {
                    let is_last = retry == self.config.max_retries - 1;
                    tracing::error!(
                        "[板块分析] {} ❌ API失败({}) 重试{}/{} {}",
                        company_name, e, retry + 1, self.config.max_retries,
                        if is_last { "(最后机会)" } else { "" }
                    );
                    if is_last {
                        let has_content = !last_content.is_empty();
                        tracing::warn!(
                            "[板块分析] {} 重试耗尽: has_content={} last_len={}",
                            company_name, has_content, last_content.len()
                        );
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
        tracing::warn!(
            "[板块分析] {} 全部重试耗尽: has_content={} last_len={}",
            company_name, has_content, last_content.len()
        );
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

        tracing::info!(
            "[经营分析] {} START | sys={}ch user={}ch threshold={} retries={}",
            company_name, system_prompt.len(), user_prompt.len(),
            self.config.quality_threshold, self.config.max_retries
        );
        tracing::debug!(
            "[经营分析] {} user_prompt:\n{}\n---end---",
            company_name, user_prompt
        );

        let mut last_content = String::new();
        let mut last_score = 0u32;
        let mut last_usage: Option<TokenUsage> = None;

        for retry in 0..self.config.max_retries {
            if retry > 0 {
                let delay_ms = (1u64 << (retry - 1)) * 1000;
                tracing::info!("[经营分析] {} 重试{}/{} 等待{}ms", company_name, retry + 1, self.config.max_retries, delay_ms);
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
            tracing::info!("[经营分析] {} 第{}次调用 API...", company_name, retry + 1);
            match self.call(system_prompt, user_prompt).await {
                Ok((content, usage)) => {
                    let content_len = content.len();
                    let qr = checker.evaluate(company_name, &content, business_type_enum);
                    let score = qr.score;
                    last_content = content;
                    last_score = score;
                    last_usage = usage.clone();

                    tracing::info!(
                        "[经营分析] {} (第{}次): 得分{}/{} | {}字 | 摘要{} 营收{} ebitda{} 现金流{} 支出{}",
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
                        tracing::info!("[经营分析] {} ✅ 达标(得分{}/{})", company_name, score, self.config.quality_threshold);
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
                        "[经营分析] {} ⚠ 不达标(得分{}/{}) 重试{}/{}",
                        company_name, score, self.config.quality_threshold,
                        retry + 1, self.config.max_retries
                    );
                    tracing::info!(
                        "[经营分析] {} 质量评分不足({}/{})，正在重试 ({}/{})...",
                        company_name, score, self.config.quality_threshold,
                        retry + 1, self.config.max_retries
                    );
                }
                Err(e) => {
                    let is_last = retry == self.config.max_retries - 1;
                    tracing::error!(
                        "[经营分析] {} ❌ API失败({}) 重试{}/{} {}",
                        company_name, e,
                        retry + 1, self.config.max_retries,
                        if is_last { "(最后机会)" } else { "" }
                    );
                    if is_last {
                        tracing::warn!("[经营分析] {} 重试耗尽, 无内容", company_name);
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

}

/// 最小兜底提示词（仅在所有文件加载路径都失败时使用）
const FALLBACK_PROMPT: &str = "你是严谨的财务分析师。基于给定经营数据输出客观精简的分析。";

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

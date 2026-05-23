/// 前端类型定义（与 Rust 后端对应）

/** 业态类型 */
export type BusinessType = 'Insurance' | 'Hotel' | 'Commercial';

/** 公司实体 */
export interface Company {
    name: string;
    business_type: BusinessType;
    regions: string[];
}

/** 项目实体 */
export interface Project {
    name: string;
    year: number;
    month: number;
    data_folder: string;
    output_file: string;
    companies: Company[];
    ytd_months: number;
    ai_config: AIConfig;
}

/** AI 配置 */
export interface AIConfig {
    api_url: string;
    api_key: string;
    model: string;
    temperature: number;
    max_tokens: number;
    system_prompt_path: string;
    batch_size: number;
    max_retries: number;
    quality_threshold: number;
}

/** 预览数据 */
export interface PreviewData {
    engine_name: string;
    files_found: string[];
    sheets_detected: string[];
    companies_detected: string[];
    available_indicators: string[];
    warnings: string[];
}

/** 汇总结果 */
export interface AggregationResult {
    engine_name: string;
    companies_processed: number;
    indicators_collected: number;
    warnings: string[];
    summary_data: string;
}

/** AI 分析结果 */
export interface AnalysisResult {
    company_name: string;
    business_type: string;
    content: string;
    quality_score: number;
    retry_count: number;
    token_usage?: TokenUsage;
    success: boolean;
    error_message?: string;
    /** 分析类别：segment=板块分析, company=公司经营指标分析 */
    analysis_category: string;
}

/** Token 用量 */
export interface TokenUsage {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
}

/** 进度更新 */
export interface ProgressUpdate {
    step: string;
    progress: number;
    status: 'running' | 'done' | 'error';
    company?: string;
}

/** 全局应用配置 */
export interface AppConfig {
    general: {
        language: string;
        theme: string;
        recent_projects: string[];
    };
    defaults: {
        default_data_folder: string;
        default_output_folder: string;
        api_url: string;
        model: string;
        system_prompt_path: string;
    };
}

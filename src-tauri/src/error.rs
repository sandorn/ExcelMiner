use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 统一错误类型
#[derive(Debug, thiserror::Error, Serialize)]
pub enum AppError {
    #[error("文件不存在: {0}")]
    FileNotFound(String),

    #[error("Sheet '{sheet}' 未在文件 '{file}' 中找到")]
    SheetNotFound { file: String, sheet: String },

    #[error("关键词未找到: {keywords:?}")]
    KeywordNotFound { keywords: Vec<String> },

    #[error("数据缺失: {0}")]
    MissingData(String),

    #[error("API 调用失败 (第{retry}次重试): {message}")]
    ApiError { retry: u32, message: String },

    #[error("质量评分不足: 得分 {score}/{threshold}")]
    QualityTooLow { score: u32, threshold: u32 },

    #[error("IO 错误: {0}")]
    Io(String),

    #[error("Excel 读取错误: {0}")]
    Excel(String),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("{0}")]
    Other(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<calamine::XlsxError> for AppError {
    fn from(e: calamine::XlsxError) -> Self {
        AppError::Excel(e.to_string())
    }
}

impl From<toml::de::Error> for AppError {
    fn from(e: toml::de::Error) -> Self {
        AppError::Config(e.to_string())
    }
}

impl From<rust_xlsxwriter::XlsxError> for AppError {
    fn from(e: rust_xlsxwriter::XlsxError) -> Self {
        AppError::Other(format!("XlsxError: {}", e))
    }
}

pub type AppResult<T> = Result<T, AppError>;

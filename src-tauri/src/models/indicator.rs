use serde::{Deserialize, Serialize};

/// 指标定义（对应汇总表中的一项指标）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorDef {
    /// 指标名称（如 "人力", "保费收入", "入住率"）
    pub name: String,
    /// 显示标签
    pub label: String,
    /// Excel 中搜索的关键词
    pub keywords: Vec<String>,
    /// 单位
    #[serde(default)]
    pub unit: String,
    /// 是否需要 YTD 累计
    #[serde(default)]
    pub need_ytd: bool,
    /// 是否为百分比
    #[serde(default)]
    pub is_percent: bool,
    /// 计算列偏移（相对关键词列）
    #[serde(default)]
    pub value_col_offset: i32,
    /// 公式类型（如 "活动率 = 活动量/人力"）
    #[serde(default)]
    pub formula: Option<String>,
}

/// 汇总后的指标值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorValue {
    pub name: String,
    pub value: f64,
    pub ytd_value: Option<f64>,
    pub unit: String,
    pub is_percent: bool,
}

/// 指标配置集合（按业态定义）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorSet {
    pub business_type: String,
    pub indicators: Vec<IndicatorDef>,
}

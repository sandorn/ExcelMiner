# ExcelMiner — 子公司经营数据汇总分析系统

## 1. 项目概述

基于 VBA 宏文件工作流的桌面软件，Windows 平台，实现子公司月度经营数据的**收集→汇总→AI分析→导出**全流程自动化。

### 技术栈

| 层         | 选型                         | 版本     |
| ---------- | ---------------------------- | -------- |
| 语言       | Rust                         | 1.80+    |
| 桌面壳     | Tauri                        | v2       |
| 前端       | React + TypeScript           | 18 + 5.x |
| 前端构建   | Vite                         | 5.x      |
| 前端UI     | Ant Design                   | 5.x      |
| Excel读取  | calamine                     | 0.24+    |
| 数据处理   | polars                       | 0.42+    |
| Excel写入  | rust_xlsxwriter              | 0.8+     |
| HTTP客户端 | reqwest                      | 0.12+    |
| 序列化     | serde + serde_json           | 1.x      |
| 错误处理   | anyhow + thiserror           | 1.x      |
| 日志       | tracing + tracing-subscriber | 0.3+     |
| 异步运行时 | tokio                        | 1.x      |

> **格式约定**：`.et`（WPS表格）文件统一要求另存为 `.xlsx` 后再导入，系统只处理 `.xlsx`。

---

## 2. 业务流程

### 业务原型文件映射 (更新后)

| VBA文件 | 功能分类 | 说明 |
|---|---|---|
| `汇总公共模块.bas` | 🔧 公共函数 | `ColLetter`, `GetTargetMonth`, `ParseNumeric`, `SafeRead`, `SumAchievementCols` |
| `DeepSeekAPI.bas` | 🔧 AI公共模块 | API调用、JSON解析、提示词构建、重试机制 |
| `保险数据汇总.bas` | 📥 数据汇总 | 保险业态：2家公司人力/保费/续期指标 |
| `商写数据汇总.bas` | 📥 数据汇总 | 商写业态：5家公司招商/渠道/续签指标 |
| `酒店数据汇总.bas` | 📥 数据汇总 | 酒店业态：2家公司营销+经营指标 |
| `经营报表汇总.bas` | 📥 数据汇总 | 全业态：通用财报指标复制（按填写页配置） |
| `业态分析整合版.bas` | 🤖 AI分析 | 一键运行3个业态分析（商写+保险+酒店） |
| `核心指标分析.bas` | 🤖 AI分析 | 逐公司深度分析（6维度质量检查） |
| `提示词_优化版_纯文本.md` | 📝 提示词 | 核心指标分析的AI系统提示词 |

```
┌─────────────┐    ┌──────────────┐    ┌──────────────┐    ┌─────────────┐
│  项目设置    │───▶│  数据汇总     │───▶│  AI业态分析   │───▶│  报表导出    │
│  填写页配置  │    │  4个汇总引擎  │    │  业态分析(3合1)│   │  汇总表.xlsx │
│  公司列表    │    │  公共模块驱动  │    │  核心指标分析  │    │  PPT文案     │
│  提示词存储  │    │  预览→执行   │    │  质量检查(6维) │    │              │
└─────────────┘    └──────────────┘    └──────────────┘    └─────────────┘
```

### 2.1 项目设置

创建/打开一个"项目"（对应一个月份的工作目录），指定：

- **月份标识**：如"2024年6月"（优先从汇总表"填写页"的A2单元格读取，回退到文件夹名解析）
- **数据文件夹**：存放各子公司 `活动量/` 和 `经营报表/` 的根目录
- **输出文件**：汇总 Excel 路径（如 `【X年X月】经营数据.xlsx`）
- **子公司清单**：按业态（保险/酒店/商写）分组配置

### 2.2 数据汇总（4个汇总引擎）

| 汇总引擎     | 业态      | 数据来源            | 汇总内容                                 |
| ------------ | --------- | ------------------- | ---------------------------------------- |
| 保险数据汇总 | 保险(2家) | 活动量/             | 人力指标、保费收入、续期指标、活动转化率 |
| 酒店数据汇总 | 酒店(2家) | 活动量/ + 经营报表/ | 营销活动、OTA评分、入住率、营收指标      |
| 商写数据汇总 | 商写(5家) | 活动量/             | 招商面积、渠道分析、续签情况、出租率     |
| 经营报表汇总 | 全业态    | 经营报表/           | 通用财务指标（利润、成本、费用等）       |

**关键计算逻辑**：

- **YTD累计**：从各月"达成列"逐月求和（非整列求和，精确到YTD对应列）
- **文本数字提取**：`"直播1736场"→1736`、`"1+1000"→1001`
- **公式自动写入**：活动率=活动量/人力，转化率=成交数/活动量，人均保费=保费/人力

### 2.3 AI 业态分析（2个分析模块，整合版）

| 分析模块 | 覆盖业态 | 数据范围 | 结果写入 | 系统提示词 |
|---|---|---|---|---|
| 业态分析整合版 | 商写+保险+酒店 | 各业态Sheet指定Range | 各业态Sheet指定Cell | Sheet内L1/M1单元格 |
| 核心指标分析 | 全业态（逐公司） | 每公司Sheet C1:R5 | 每公司Sheet C61 | 填写页!C20 |

**业态分析整合版关键配置**：
- 商写：数据 A1:G18 → 结果 L14，提示词 L1
- 保险：数据 F2:H25 → 结果 L14，提示词 L1（含月份过滤逻辑）
- 酒店：三区域 B1:D5 / E1:G13 / I1:K13 → 结果 M14，提示词 M1

**核心指标分析关键特性**：
- 逐公司单独分析（每个子公司Sheet读取C1:R5数据）
- 公司列表从填写页C2:C10读取
- **6维度质量检查**（摘要/营收/EBITDA/现金流/支出，每项2分，满分10）
- 低于8分自动重试（最多2次）
- 序时进度从填写页A8读取
- 分析年份从填写页A4读取
- 系统提示词从填写页C20读取

### 2.4 报表导出

- 汇总数据写入"汇总工作簿"的各 Sheet
- AI分析结果写入对应公司/业态的 Sheet
- PPT文案可复制粘贴到演示文稿

---

## 3. 项目结构

```
ExcelMiner/
├── DESIGN.md                         # 本文档
├── README.md                         # 使用说明
│
├── src-tauri/                        # Rust 后端
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json
│   ├── icons/                        # 应用图标
│   └── src/
│       ├── main.rs                   # 入口：Tauri 启动
│       ├── lib.rs                    # 模块注册 + Tauri 命令注册
│       ├── error.rs                  # 统一错误类型
│       ├── config.rs                 # 配置管理（TOML 读写）
│       │
│       ├── models/                   # 数据模型
│       │   ├── mod.rs
│       │   ├── project.rs            # Project, 月份, 文件夹路径
│       │   ├── company.rs            # Company(名称/业态/类型)
│       │   ├── indicator.rs          # IndicatorDef(名称/列映射/公式)
│       │   └── analysis.rs           # AnalysisResult(公司/内容/评分)
│       │
│       ├── services/                 # 核心业务逻辑
│       │   ├── mod.rs
│       │   ├── excel_reader.rs       # calamine 封装, 读 xlsx
│       │   ├── number_parser.rs      # 文本数字提取
│       │   ├── data_aggregator.rs    # 4个汇总引擎
│       │   │   ├── insurance.rs      # 保险数据汇总
│       │   │   ├── hotel.rs          # 酒店数据汇总
│       │   │   ├── commercial.rs     # 商写数据汇总
│       │   │   └── financial.rs      # 经营报表汇总
│       │   ├── ai_analyzer.rs        # DeepSeek API 调用封装
│       │   │   ├── client.rs         # HTTP 客户端
│       │   │   ├── prompt.rs         # 提示词构建
│       │   │   └── retry.rs          # 重试 + 质量检查
│       │   ├── quality_checker.rs    # 自评分校验（8/10阈值）
│       │   └── report_writer.rs      # 写入汇总 xlsx
│       │
│       ├── commands/                 # Tauri IPC 命令
│       │   ├── mod.rs
│       │   ├── project_cmd.rs        # 项目 CRUD
│       │   ├── import_cmd.rs         # 数据预览 + 执行汇总
│       │   ├── analysis_cmd.rs       # 执行 AI 分析
│       │   └── export_cmd.rs         # 导出报表
│       │
│       └── utils/
│           ├── mod.rs
│           └── date_utils.rs         # 月份解析、YTD 计算
│
├── src/                              # React 前端 (Tauri 默认结构)
│   ├── main.tsx                      # 入口
│   ├── App.tsx                       # 路由 + 布局
│   ├── vite-env.d.ts
│   │
│   ├── pages/                        # 页面
│   │   ├── ProjectSetup.tsx          # 步骤1：项目设置
│   │   ├── DataImport.tsx            # 步骤2：数据汇总
│   │   ├── AIAnalysis.tsx            # 步骤3：AI分析
│   │   └── ReportExport.tsx          # 步骤4：报表导出
│   │
│   ├── components/                   # 通用组件
│   │   ├── Layout.tsx                # 全局布局（侧边导航+工作区）
│   │   ├── StepsProgress.tsx         # 步骤条
│   │   ├── FileTree.tsx              # 文件夹浏览
│   │   ├── DataPreview.tsx           # 数据预览表格
│   │   ├── ProgressPanel.tsx         # 进度面板
│   │   ├── ResultCard.tsx            # AI分析结果卡片
│   │   └── ConfigEditor.tsx          # 配置编辑器
│   │
│   ├── hooks/                        # 自定义 hooks
│   │   ├── useProject.ts
│   │   ├── useImport.ts
│   │   └── useAnalysis.ts
│   │
│   ├── stores/                       # 状态管理
│   │   └── appStore.ts
│   │
│   ├── types/                        # TypeScript 类型
│   │   └── index.ts
│   │
│   └── styles/                       # 样式
│       └── global.css
│
├── resources/                        # 静态资源
│   └── prompts/
│       └── 财务分析师.md              # AI 系统提示词
│
├── tests/                            # Rust 测试
│   ├── test_number_parser.rs
│   ├── test_data_aggregator.rs
│   └── test_quality_checker.rs
│
├── package.json                      # 前端依赖
├── tsconfig.json
├── vite.config.ts
└── index.html
```

---

## 4. 核心 Rust 模块设计

### 4.1 数据模型 (`models/`)

```rust
// models/project.rs

/// 业态枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BusinessType {
    Insurance,   // 保险
    Hotel,       // 酒店
    Commercial,  // 商写
}

/// 公司实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Company {
    pub name: String,
    pub business_type: BusinessType,
    pub regions: Vec<String>,  // 区域（酒店业态用）
}

/// 项目配置（对应一个月份的工作目录）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,                   // e.g. "2024年6月"
    pub year: u32,
    pub month: u32,
    pub data_folder: PathBuf,           // 子公司数据根目录
    pub output_file: PathBuf,           // 汇总输出 .xlsx
    pub companies: Vec<Company>,
    pub ytd_months: u32,                // YTD累计月份数
    pub ai_config: AIConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: u32,
    pub system_prompt_path: PathBuf,
    pub batch_size: usize,          // 每批公司数
    pub max_retries: u32,
    pub quality_threshold: u32,     // 质量阈值, 默认 8
}
```

### 4.2 Excel 读取 (`services/excel_reader.rs`)

```rust
/// calamine 封装，提供统一读取接口
pub struct ExcelReader {
    workbook: calamine::Xlsx<BufReader<File>>,
}

impl ExcelReader {
    /// 打开 .xlsx 文件
    pub fn open(path: &Path) -> Result<Self>;

    /// 获取所有 Sheet 名称
    pub fn sheet_names(&self) -> Vec<String>;

    /// 读取指定 Sheet 为二维矩阵 (Vec<Vec<CellValue>>)
    pub fn read_sheet(&mut self, name: &str) -> Result<SheetData>;

    /// 读取指定单元格的值（支持解析公式缓存值）
    pub fn read_cell(&mut self, sheet: &str, row: u32, col: u32) -> Result<CellValue>;

    /// 读取指定行
    pub fn read_row(&mut self, sheet: &str, row: u32) -> Result<Vec<CellValue>>;

    /// 读取指定列
    pub fn read_column(&mut self, sheet: &str, col: u32) -> Result<Vec<CellValue>>;

    /// 搜索包含关键词的单元格，返回其行列
    pub fn find_keyword(&mut self, sheet: &str, keywords: &[&str]) -> Result<Vec<(u32, u32)>>;
}

pub struct SheetData {
    pub headers: Vec<String>,       // 第一行作为表头
    pub rows: Vec<Vec<CellValue>>,  // 数据行
    pub dimensions: (u32, u32),     // (行数, 列数)
}
```

### 4.3 数字解析 (`services/number_parser.rs`)

```rust
/// 从文本中提取数字，处理各种格式
pub struct NumberParser;

impl NumberParser {
    /// 主入口：从单元格值提取数字
    /// "直播1736场" → 1736.0
    /// "1+1000"    → 1001.0
    /// "¥1,234.56" → 1234.56
    /// "85%"        → 0.85
    /// 纯中文/无数字 → None
    pub fn parse(value: &str) -> Option<f64>;

    /// 处理 "a+b" 格式的求和表达式
    fn eval_expression(text: &str) -> Option<f64>;

    /// 处理百分比
    fn parse_percent(text: &str) -> Option<f64>;

    /// 清理千分位逗号、货币符号等
    fn clean_number_text(text: &str) -> String;
}
```

### 4.4 数据汇总引擎 (`services/data_aggregator/`)

```rust
/// 汇总引擎 trait
#[async_trait]
pub trait AggregationEngine {
    /// 返回引擎名称
    fn name(&self) -> &str;

    /// 预览：读取文件，返回可用字段供 UI 展示
    fn preview(&self, project: &Project) -> Result<PreviewData>;

    /// 执行汇总：读取数据 → 计算 → 写入汇总表
    async fn execute(&self, project: &Project, writer: &mut ReportWriter) -> Result<AggregationResult>;
}

pub struct AggregationResult {
    pub engine_name: String,
    pub companies_processed: usize,
    pub indicators_collected: usize,
    pub warnings: Vec<String>,      // 数据缺失等警告
    pub ytd_sums: HashMap<String, HashMap<String, f64>>,  // 公司→指标→YTD值
}

pub struct PreviewData {
    pub files_found: Vec<PathBuf>,
    pub sheets_detected: Vec<String>,
    pub companies_detected: Vec<String>,
    pub available_indicators: Vec<String>,
}
```

### 4.5 AI 分析引擎 (`services/ai_analyzer/`)

```rust
pub struct AIAnalyzer {
    config: AIConfig,
    client: reqwest::Client,
}

impl AIAnalyzer {
    pub fn new(config: AIConfig) -> Self;

    /// 加载系统提示词
    pub fn load_system_prompt(&self) -> Result<String>;

    /// 单次调用 DeepSeek API
    pub async fn call(&self, system_prompt: &str, user_prompt: &str) -> Result<String>;

    /// 分批分析（带重试和质量检查）
    pub async fn analyze_batch(
        &self,
        business_type: BusinessType,
        companies_data: &[(Company, String)],  // (公司, 汇总数据文本)
        on_progress: impl Fn(ProgressUpdate),
    ) -> Result<Vec<AnalysisResult>>;

    /// 自评分检查
    pub async fn check_quality(&self, content: &str) -> Result<u32>;
}

pub struct AnalysisResult {
    pub company_name: String,
    pub business_type: BusinessType,
    pub content: String,            // AI 返回的分析文案
    pub quality_score: u32,         // 自评分
    pub retry_count: u32,
    pub token_usage: Option<TokenUsage>,
}

pub struct ProgressUpdate {
    pub step: String,               // "正在分析 保险业态 → 子公司A (第1/2批)"
    pub progress: f32,              // 0.0 ~ 1.0
    pub status: ProgressStatus,     // Running / Done / Error
}
```

### 4.6 Tauri 命令 (`commands/`)

```rust
// commands/project_cmd.rs
#[tauri::command]
async fn create_project(config: ProjectConfig) -> Result<Project, AppError>;

#[tauri::command]
async fn open_project(path: String) -> Result<Project, AppError>;

#[tauri::command]
async fn save_project(project: Project) -> Result<(), AppError>;

// commands/import_cmd.rs
#[tauri::command]
async fn preview_import(project: Project, engine: String) -> Result<PreviewData, AppError>;

#[tauri::command]
async fn execute_aggregation(
    project: Project,
    engines: Vec<String>,         // 可选择运行哪些引擎
    window: tauri::Window,        // 用于 emit 进度事件
) -> Result<Vec<AggregationResult>, AppError>;

// commands/analysis_cmd.rs
#[tauri::command]
async fn execute_analysis(
    project: Project,
    business_types: Vec<BusinessType>,
    window: tauri::Window,
) -> Result<Vec<AnalysisResult>, AppError>;

// commands/export_cmd.rs
#[tauri::command]
async fn export_report(project: Project) -> Result<PathBuf, AppError>;
```

---

## 5. 前端设计

### 5.1 页面路由

```
/                     → 重定向到 /setup
/setup                → 项目设置页
/import               → 数据汇总页
/analysis             → AI分析页
/export               → 报表导出页
```

### 5.2 全局布局

```
┌──────────────────────────────────────────────────┐
│  ExcelMiner                          [设置] [×]   │
├────────┬─────────────────────────────────────────┤
│        │  ● 项目设置                              │
│  ◉ ①   │  ○ 数据汇总                              │
│  ○ ②   │  ○ AI分析                                │
│  ○ ③   │  ○ 报表导出                              │
│  ○ ④   │                                         │
│        │  ┌─────────────────────────────────────┐│
│ 左侧   │  │                                     ││
│ 步骤   │  │        当前步骤的工作区               ││
│ 导航   │  │                                     ││
│        │  │                                     ││
│        │  └─────────────────────────────────────┘│
│        │                                         │
│        │         [上一步]      [下一步]           │
└────────┴─────────────────────────────────────────┘
```

### 5.3 页面设计要点

| 页面         | 核心交互                                                                                                                                           |
| ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| **项目设置** | 月份选择器、文件夹选择按钮（调用 Tauri 原生对话框）、公司列表勾选、业态分组编辑                                                                    |
| **数据汇总** | 4个汇总引擎的卡片（勾选+状态灯）、"预览"按钮展示发现的数据、"一键汇总"按钮+实时进度条、汇总结果数据预览表                                          |
| **AI分析**   | 提示词编辑器（加载/保存 .md）、API Key 配置（加密存储）、3个业态勾选（可单选/全选）、执行进度（当前公司+批次）、结果卡片（可折叠，显示内容和评分） |
| **报表导出** | 汇总表预览（只读）、"打开输出文件夹"、"导出PPT文案"（复制到剪贴板）                                                                                |

### 5.4 前后端通信

使用 Tauri v2 的 `invoke` + `event` 机制：

```typescript
// 调用 Rust 命令
const result = await invoke<PreviewData>('preview_import', {
    project: currentProject,
    engine: 'insurance',
});

// 监听进度事件
listen<ProgressUpdate>('aggregation-progress', (event) => {
    setProgress(event.payload.progress);
    setStatusText(event.payload.step);
});
```

---

## 6. 配置管理

### 6.1 项目配置文件

每个"月份项目"保存为一个 `.toml` 文件，结构如下：

```toml
[project]
name = "2024年6月"
year = 2024
month = 6
data_folder = "D:/经营数据/2024年6月/"
output_file = "D:/经营数据/汇总/【2024年6月】经营数据.xlsx"
ytd_months = 6

[[project.companies]]
name = "子公司A"
business_type = "Insurance"

[[project.companies]]
name = "子公司B"
business_type = "Insurance"

[[project.companies]]
name = "子公司C"
business_type = "Hotel"
regions = ["餐饮", "客房", "会议"]

[ai]
api_url = "https://api.deepseek.com/v1/chat/completions"
api_key = ""  # 运行时输入，不保存
model = "deepseek-chat"
temperature = 0.3
max_tokens = 4096
system_prompt_path = "resources/prompts/财务分析师.md"
batch_size = 3
max_retries = 3
quality_threshold = 8
```

### 6.2 应用全局配置

```toml
# ~/AppData/Roaming/ExcelMiner/config.toml
[general]
language = "zh-CN"
theme = "light"          # light | dark
recent_projects = [
    "D:/经营数据/汇总/2024年6月.project.toml",
]

[defaults]
default_data_folder = "D:/经营数据/"
default_output_folder = "D:/经营数据/汇总/"
```

---

## 7. 错误处理策略

```rust
/// 统一错误类型
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("文件不存在: {0}")]
    FileNotFound(PathBuf),

    #[error("Sheet '{sheet}' 未在文件 '{file}' 中找到")]
    SheetNotFound { file: String, sheet: String },

    #[error("在 '{sheet}' 的 ({row},{col}) 处找不到关键词: {keywords:?}")]
    KeywordNotFound { sheet: String, row: u32, col: u32, keywords: Vec<String> },

    #[error("数据缺失: {0}")]
    MissingData(String),

    #[error("API 调用失败 (第{retry}次重试): {message}")]
    ApiError { retry: u32, message: String },

    #[error("质量评分不足: 得分 {score}/{threshold}")]
    QualityTooLow { score: u32, threshold: u32 },

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("Excel 读取错误: {0}")]
    Excel(#[from] calamine::Error),

    #[error("Excel 写入错误: {0}")]
    XlsxWriter(#[from] rust_xlsxwriter::XlsxError),

    #[error("{0}")]
    Other(String),
}

// Tauri 命令返回类型
pub type AppResult<T> = std::result::Result<T, AppError>;

// 实现 Into<tauri::InvokeError> 以支持 Tauri 自动错误转换
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: serde::Serializer {
        serializer.serialize_str(&self.to_string())
    }
}
```

---

## 8. 关键依赖 (`Cargo.toml`)

```toml
[package]
name = "excelminer"
version = "0.1.0"
edition = "2021"

[dependencies]
# Tauri
tauri = { version = "2", features = ["devtools"] }
tauri-plugin-dialog = "2"
tauri-plugin-fs = "2"
tauri-plugin-shell = "2"

# Excel
calamine = "0.24"
rust_xlsxwriter = "0.8"

# 数据处理
polars = { version = "0.42", features = ["lazy", "fmt"] }

# 异步 + HTTP
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }

# 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# 错误处理
anyhow = "1"
thiserror = "1"

# 日志
tracing = "0.1"
tracing-subscriber = "0.3"

# 工具
chrono = "0.4"
regex = "1"
uuid = { version = "1", features = ["v4"] }

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dev-dependencies]
rstest = "0.22"
tempfile = "3"
```

---

## 9. 开发阶段划分

### Phase 1：基础框架（3-5天）

- [ ] Rust 项目初始化、Tauri 壳搭建
- [ ] React + Ant Design 前端框架
- [ ] 全局布局（步骤导航 + 工作区）
- [ ] 配置管理（读/写 TOML）
- [ ] 统一错误类型 + 日志

### Phase 2：数据汇总引擎（5-8天）

- [ ] Excel 读取封装（calamine）
- [ ] 数字解析器（number_parser）
- [ ] 保险数据汇总引擎
- [ ] 酒店数据汇总引擎
- [ ] 商写数据汇总引擎
- [ ] 经营报表汇总引擎
- [ ] YTD 累计计算
- [ ] 数据预览 + 执行进度事件

### Phase 3：AI 分析引擎（3-5天）

- [ ] DeepSeek API 客户端
- [ ] 系统提示词加载与构建
- [ ] 3个业态分析模块
- [ ] 分批 + 重试 + 质量检查
- [ ] AI分析结果写入汇总表
- [ ] 前端进度展示 + 结果卡片

### Phase 4：报表导出 + UI 完善（3-5天）

- [ ] 报表写入（rust_xlsxwriter）
- [ ] PPT文案导出（复制到剪贴板）
- [ ] 主题、样式完善
- [ ] 全部页面联调

### Phase 5：测试 + 打包（2-3天）

- [ ] 单元测试（number_parser, aggregator, quality_checker）
- [ ] 集成测试（端到端工作流）
- [ ] MSI/NSIS 安装包配置
- [ ] README + 用户文档

---

## 10. 数据流示意

```
子公司 .xlsx 文件
       │
       ▼
┌─────────────────┐
│   calamine      │  只读解析
│   (ExcelReader) │
└────────┬────────┘
         │ Vec<Vec<CellValue>>
         ▼
┌─────────────────┐
│  NumberParser   │  文本→数字
│  + DataAggr     │  汇总计算
└────────┬────────┘
         │ HashMap<公司, HashMap<指标, 值>>
         ▼
┌─────────────────┐   ┌─────────────────┐
│  report_writer  │   │  ai_analyzer    │
│  → 汇总 .xlsx   │   │  → DeepSeek API │
└─────────────────┘   └────────┬────────┘
                               │
                               ▼
                      ┌─────────────────┐
                      │  质量检查        │
                      │  score >= 8?    │
                      │  Yes→保存        │
                      │  No →重试(≤3次) │
                      └────────┬────────┘
                               │
                               ▼
                      ┌─────────────────┐
                      │  report_writer  │
                      │  → 汇总 .xlsx   │
                      │  (AI分析sheet)  │
                      └─────────────────┘
```

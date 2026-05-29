# ExcelMiner 架构设计文档

## 1. 项目概述

ExcelMiner 是基于 Tauri v2 的 Windows 桌面应用，用于多子公司 Excel 经营数据自动汇总与 DeepSeek AI 经营分析。

| 层         | 技术栈                                                    |
| ---------- | --------------------------------------------------------- |
| 桌面壳     | Tauri v2（Rust）                                          |
| 前端       | React 18 + TypeScript + Ant Design 5                      |
| 状态管理   | Zustand 5                                                 |
| Excel 读取 | calamine 0.26                                             |
| Excel 写入 | umya-spreadsheet 2.3（加载已有模板 + 原地写入指定单元格） |
| HTTP       | reqwest 0.12（rustls-tls）                                |
| AI         | DeepSeek Chat API                                         |
| 构建产物   | `release-portable/` 便携版                                |

## 2. 业务流程

```
Step 1: 项目设置 → Step 2: 数据汇总 → Step 3: AI分析 → Step 4: 报表导出
```

1. **项目设置** — 输入项目名称+年月、选择数据/输出文件夹、勾选子公司、配置AI参数
2. **数据汇总** — 按业态引擎（保险/酒店/商写/经营报表）自动识别Excel数据并汇总YTD指标
3. **AI分析** — 调用DeepSeek API并发生成经营分析（Semaphore(18)），含4维度质量评分+最多2次重试（摘要不计分）
4. **报表导出** — 写入xlsx汇总文件 + PPT文案复制到剪贴板

## 3. 项目结构

```
ExcelMiner/
├── src/                          # React 前端
│   ├── main.tsx                  # 入口
│   ├── App.tsx                   # 根组件（路由+布局）
│   ├── pages/
│   │   ├── ProjectSetup.tsx      # Step 1 — 项目设置
│   │   ├── MainPage.tsx          # **主界面** — 单页面一体化操作
│   │   ├── ProjectSetup.tsx      # [旧] Step 1 — 项目设置（保留兼容）
│   │   ├── DataImport.tsx        # [旧] Step 2 — 数据汇总（保留兼容）
│   │   ├── AIAnalysis.tsx        # [旧] Step 3 — AI分析（保留兼容）
│   │   └── ReportExport.tsx      # [旧] Step 4 — 报表导出（保留兼容）
│   ├── stores/appStore.ts        # Zustand 全局状态
│   ├── types/index.ts            # TypeScript 类型定义
│   └── styles/                   # 全局样式
├── src-tauri/                    # Rust 后端
│   ├── src/
│   │   ├── main.rs               # 可执行入口
│   │   ├── lib.rs                # Tauri Builder + 命令注册 + 日志初始化
│   │   ├── config.rs             # AppConfig / GeneralConfig / DefaultConfig
│   │   ├── error.rs              # AppError（10变体统一错误类型）
│   │   ├── commands/
│   │   │   ├── project_cmd.rs    # 项目CRUD（4个命令）
│   │   │   ├── import_cmd.rs     # 数据导入（2个命令）
│   │   │   ├── analysis_cmd.rs   # AI分析（5个命令）
│   │   │   └── export_cmd.rs     # 报表导出（4个命令）
│   │   ├── models/
│   │   │   ├── project.rs        # Project / Company / BusinessType / AIConfig
│   │   │   ├── company.rs        # Company 重导出
│   │   │   ├── indicator.rs      # IndicatorDef / IndicatorValue / IndicatorSet
│   │   │   └── analysis.rs       # AnalysisResult / AnalysisQuality / TokenUsage / ProgressUpdate / PreviewData / AggregationResult
│   │   ├── services/
│   │   │   ├── data_aggregator.rs         # AggregationEngine trait + EngineType 枚举
│   │   │   ├── data_aggregator/
│   │   │   │   ├── insurance.rs           # 保险汇总引擎
│   │   │   │   ├── hotel.rs               # 酒店汇总引擎
│   │   │   │   ├── commercial.rs          # 商写汇总引擎
│   │   │   │   └── financial.rs           # 经营报表汇总引擎
│   │   │   ├── ai_analyzer.rs             # DeepSeek API 调用 + 分批重试 + 质量评分
│   │   │   ├── company_registry.rs        # 从 companies.toml 加载公司模板
│   │   │   ├── excel_reader.rs            # calamine 泛型封装 ExcelReader<RS>
│   │   │   ├── number_parser.rs           # 文本→数字解析（含表达式求值）
│   │   │   ├── quality_checker.rs         # 分析内容验证 + 质量评估（QualityChecker 结构体）
│   │   │   └── report_writer.rs           # xlsx 报表写入
│   │   └── utils/
│   │       ├── mod.rs                     # utils 模块入口
│   │       └── date_utils.rs              # 日期解析（年月/文件夹名）+ YTD月份计算
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── tests/
│       ├── test_core.rs                   # 核心功能测试（number_parser/quality_checker/date_utils/ai_analyzer）
│       ├── test_aggregation.rs            # 数据汇总测试
│       └── test_xlsx_debug.rs             # Excel 文件读取调试
├── resources/
│   ├── companies.toml            # 子公司预定义模板（9家公司3个业态）
│   └── prompts/                  # AI 系统提示词（保险分析/酒店分析/商写分析/财务分析师）
├── docs/                         # 项目文档
│   ├── 用户操作手册.md            # 最终用户操作指南
│   └── 业务逻辑详解.md            # 汇总规则与指标计算说明
├── 业务原型/                     # 原始 VBA 脚本参考（不参与构建）
└── release-portable/              # 构建产物
```

## 4. 前端架构

当前版本采用**单页面一体化模式**（`MainPage.tsx`）：

```
┌────────────────────────────────────────────┐
│  App.tsx (Layout: Header + Content)        │
│    └── MainPage.tsx                        │
│          ├── 配置区（年月/目录/API Key）     │
│          ├── 操作区（一键汇总/板块分析/公司分析）│
│          └── 日志区（实时滚动输出）          │
└────────────────────────────────────────────┘
```

> 早期版本使用 4 步向导路由（`/setup → /import → /analysis → /export`），对应页面 `ProjectSetup/DataImport/AIAnalysis/ReportExport.tsx` 保留在代码库中作为兼容参考。

## 5. Rust 后端核心设计

### 5.1 AppState（全局状态）

定义在 `commands/project_cmd.rs`，通过 Tauri 的 `manage()` 注入：

```rust
pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub current_project: Mutex<Option<Project>>,
    pub aggregation_results: Mutex<Vec<AggregationResult>>,  // 跨步骤共享
    pub analysis_results: Mutex<Vec<AnalysisResult>>,        // 跨步骤共享
    pub _log_guard: Mutex<Option<WorkerGuard>>,              // 日志文件 guard
}
```

### 5.2 统一错误类型（error.rs）

```rust
pub enum AppError {
    FileNotFound(String),                                                 // 文件不存在
    SheetNotFound { file: String, sheet: String },                        // Sheet未找到
    KeywordNotFound { keywords: Vec<String> },                            // 关键词未找到
    MissingData(String),                                                  // 数据缺失
    ApiError { retry: u32, message: String },                             // API调用失败
    QualityTooLow { score: u32, threshold: u32 },                         // 质量评分不足
    Io(String),                                                           // IO错误
    Excel(String),                                                        // Excel读取错误
    Config(String),                                                       // 配置错误
    Other(String),                                                        // 其他错误
}
```

类型别名：`pub type AppResult<T> = Result<T, AppError>;`

`From` trait 自动转换：`std::io::Error` → `Io`，`calamine::XlsxError` → `Excel`，`toml::de::Error` → `Config`，`umya_spreadsheet::XlsxError` → `Other`。

### 5.3 汇总引擎（data_aggregator）

```rust
pub trait AggregationEngine {
    fn aggregate(&self, project: &Project, sender: Sender<ProgressUpdate>) -> AppResult<AggregationResult>;
}

pub enum EngineType {
    Insurance,  // 保险引擎
    Hotel,      // 酒店引擎
    Commercial, // 商写引擎
    Financial,  // 经营报表引擎
}
```

4个引擎实现同一 trait，通过 `window.emit("aggregation-progress")` 向前端推送进度。关键计算逻辑：

- **数字解析**：`NumberParser` — 支持千分位、百分号、金额前缀（¥/$）、表达式求值（如 "1+1000" → 1001）
- **YTD 累计**：按月份累加指标，使用 `utils/date_utils.rs` 中的 `ytd_months()` 计算月份序列
- **公式写入**：用 `umya_spreadsheet` 的 `set_formula()` 写入 Excel 公式
- **文件发现**：`ExcelReader<RS>` 通过 calamine 扫描目录发现 xlsx/xls 文件

### 5.4 AI 分析引擎（ai_analyzer.rs）

```rust
pub struct AIAnalyzer {
    client: reqwest::Client,    // 60s 超时
    api_url: String,
    api_key: String,
    model: String,
    temperature: f64,           // 默认 0.3
    max_tokens: u32,            // 默认 1500
    system_prompt: String,
    batch_size: usize,          // 默认 3
    max_retries: u32,           // 默认 2
    quality_threshold: u32,     // 默认 8（0-10分）
}
```

提示词加载优先级：

1. 用户指定文件路径 → 直接加载
2. 路径为空 → 按 `business_type` 返回内置默认提示词
3. `business_type` 为 None → 通用财务分析师提示词

两阶段分析流程：

**阶段一：板块业态分析**（`analyze_segment`）

```
按业态分组公司 → 加载业态专属提示词 → 嵌入对应业态汇总数据
   → 调用 DeepSeek API → 仅内容长度校验（≥50字）→ 跳过质量评分
```

**阶段二：子公司经营指标分析**（`analyze_single`）

```
逐公司遍历 → 加载财务分析师提示词 → 嵌入经营报表汇总数据
   → 调用 DeepSeek API → 4维度质量评分（摘要不计分） →
       score >= 阈值（默认8）→ 保存结果
       score < 阈值   → 重试（通过 QualityChecker，默认最多2次，指数退避延迟）
```

关键方法：

- `analyze_segment(system_prompt, user_prompt, segment_name, business_type)` — 板块级分析，无质量检查
- `analyze_single(system_prompt, user_prompt, company_name, business_type_display, business_type_enum, analysis_category)` — 单公司分析，含完整质量评分+重试
- `analyze_batch(company_data, business_type, on_progress)` — 批量分析（已废弃，保留兼容）

数据隔离：

- 板块分析仅使用**对应业态引擎**的汇总数据（保险/酒店/商写）
- 公司分析仅使用**经营报表引擎**的汇总数据

并发执行（公司分析阶段）：

- 使用 `tokio::task::JoinSet` + `tokio::sync::Semaphore(18)` 实现全并发
- 进度追踪通过 `Arc<AtomicUsize>` 计数，`window.emit("analysis-progress")` 推送
- AIAnalyzer 通过 `Arc<AIAnalyzer>` 在多任务间安全共享

### 5.5 质量评分体系（AnalysisQuality + QualityChecker）

**AnalysisQuality**（定义于 `models/analysis.rs`）：
4维度评估，每项2分，满分8分（摘要不计入评分维度）：

| 维度           | 检测内容                         | 默认业态检测                                  |
| -------------- | -------------------------------- | --------------------------------------------- |
| `has_revenue`  | 营业收入                         | 保险:人力/承保、酒店:营销/OTA、商写:整体/合作 |
| `has_ebitda`   | EBITDA / GOP / 扣非净利润 三选一 | 保险:保费规模、酒店:评价评分、商写:渠道       |
| `has_cashflow` | 经营活动净现金流                 | 保险:新单、酒店:入住率、商写:招商             |
| `has_expense`  | 经营支出                         | 保险:无、酒店:无、商写:续租                   |
| `has_summary`  | 首行摘要（不计分，仅记录）       | 通用                                          |

**QualityChecker**（定义于 `services/quality_checker.rs`）：

- `evaluate(company_name, content) → QualityResult { score, passed, reason }`
- `is_valid_content(content) → bool` — 最少50字符
- `quality_hint(quality) → String` — 生成质量提示文本
- `max_retries() → u32` — 返回允许重试次数（threshold/4，即 8/4=2）

### 5.6 日期工具模块（utils/date_utils.rs）

```rust
pub fn parse_month(text: &str) -> Option<(u32, u32)>;       // "2024年6月"/"2024-06"
pub fn ytd_months(year, month, count) -> Vec<(u32, u32)>;    // YTD月份序列（支持跨年）
pub fn parse_date_from_folder(folder_name: &str) -> Option<(u32, u32)>;  // 从文件夹名解析
```

### 5.7 报表导出（report_writer.rs）

`ReportWriter::write_summary()` 使用 `umya-spreadsheet` 2.3 加载已有模板文件，按引擎类型向指定单元格写入汇总数据和公式：

- **汇总数据 Sheet**：格式化表格（表头样式、列宽自适应）
- **AI分析 Sheet**：分析文本 + Token 用量
- **指标 Sheet**：按公司分组的指标明细

## 6. Tauri 命令清单（15个）

| 分组     | 命令                       | 参数                                                  | 返回                     | 说明                                     |
| -------- | -------------------------- | ----------------------------------------------------- | ------------------------ | ---------------------------------------- |
| project  | `create_project`           | state, name, year, month, data_folder, output_file    | `Project`                | 生成 .project.toml                       |
| project  | `open_project`             | state, path                                           | `Project`                | 反序列化 .project.toml                   |
| project  | `save_project`             | state, project                                        | `()`                     | 序列化写入 .project.toml                 |
| project  | `get_default_config`       | state                                                 | `AppConfig`              | 返回全局配置                             |
| import   | `preview_import`           | project, engine                                       | `PreviewData`            | 预览引擎发现的数据                       |
| import   | `execute_aggregation`      | state, project, engines, window                       | `Vec<AggregationResult>` | emit progress 事件                       |
| analysis | `execute_segment_analysis` | state, project, business_types, custom_prompt, window | `Vec<AnalysisResult>`    | 阶段一：板块业态分析（跳过质量检查）     |
| analysis | `execute_company_analysis` | state, project, window                                | `Vec<AnalysisResult>`    | 阶段二：子公司经营指标分析（带质量检查） |
| analysis | `execute_analysis`         | state, project, business_types, custom_prompt, window | `Vec<AnalysisResult>`    | 两阶段完整分析（板块+公司）              |
| analysis | `test_api_connection`      | api_url, api_key, model                               | `String`                 | 测试API连通性（返回"连接成功"）          |
| analysis | `read_dskey`               | section                                               | `Option<String>`         | 从 ~/.dskey 读取 API Key                 |
| export   | `export_report`            | state                                                 | `String`(路径)           | 写入xlsx                                 |
| export   | `copy_to_clipboard`        | app_handle, text                                      | `()`                     | 复制PPT文案到剪贴板                      |
| export   | `open_in_explorer`         | path                                                  | `()`                     | 打开文件浏览器定位                       |
| export   | `open_log_folder`          | (无)                                                  | `String`(日志路径)       | 打开日志目录（Shell::open）              |

## 7. 前端设计

### 7.1 Zustand 状态管理（appStore.ts）

```typescript
interface AppState {
    project: Project | null;
    projectName: string;
    appConfig: AppConfig | null;
    currentStep: number; // 0-3
    aggregationResults: any[];
    analysisResults: any[];
    // Actions: setProject / setAppConfig / setCurrentStep /
    //          setAggregationResults / setAnalysisResults
}
```

### 7.2 前后端通信

```typescript
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// 调用 Rust 命令
const preview = await invoke<PreviewData>('preview_import', { project, engine });

// 监听进度事件
listen<ProgressUpdate>('aggregation-progress', (event) => { ... });
listen<ProgressUpdate>('analysis-progress', (event) => { ... });
```

### 7.3 各页面核心交互

| 页面         | 核心功能                                                                                    |
| ------------ | ------------------------------------------------------------------------------------------- |
| ProjectSetup | 表单输入+原生文件夹选择对话框+公司勾选列表+AI参数配置                                       |
| DataImport   | 4引擎卡片（勾选+状态灯）、预览按钮→展示数据、一键汇总→实时进度条→结果表格                   |
| AIAnalysis   | API Key配置（密码框）、提示词编辑/加载、业态多选、执行进度、结果卡片（折叠展开+评分+Token） |
| ReportExport | 导出xlsx按钮、复制PPT文案按钮、打开输出目录、查看日志                                       |

## 8. 配置管理

### 8.1 项目配置（\*.project.toml）

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

[project.ai]
api_url = "https://api.deepseek.com/v1/chat/completions"
model = "deepseek-chat"
temperature = 0.3
max_tokens = 1500
system_prompt_path = "resources/prompts/财务分析师.md"
batch_size = 3
max_retries = 2
quality_threshold = 8
```

> **注意**：`Project` 与 `ProjectConfig` 为独立的序列化模型，`business_type` 在TOML中为字符串（"Insurance"/"Hotel"/"Commercial"），在内存中为 `BusinessType` 枚举。

### 8.2 全局配置（%APPDATA%/ExcelMiner/config.toml）

```toml
[general]
language = "zh-CN"
theme = "light"
recent_projects = []

[defaults]
default_data_folder = "D:/经营数据/"
default_output_folder = "D:/经营数据/汇总/"
api_url = "https://api.deepseek.com/v1/chat/completions"
model = "deepseek-chat"
system_prompt_path = ""
```

### 8.3 公司模板（resources/companies.toml）

预定义9家子公司，按三业态分组：保险（3家）、酒店含区域划分（3家）、商写（3家）。

### 8.4 AI系统提示词（resources/prompts/）

| 文件            | 用途                       |
| --------------- | -------------------------- |
| `保险分析.md`   | 保险业态专用提示词         |
| `酒店分析.md`   | 酒店业态专用提示词         |
| `商写分析.md`   | 商写业态专用提示词         |
| `财务分析师.md` | 通用财务分析提示词（兜底） |

## 9. 关键依赖

### Rust（Cargo.toml）

```toml
tauri = { version = "2", features = ["devtools"] }
tauri-plugin-dialog = "2"        # 原生文件对话框
tauri-plugin-fs = "2"            # 文件系统访问
tauri-plugin-shell = "2"         # 打开外部程序
tauri-plugin-clipboard-manager = "2"  # 剪贴板操作
calamine = "0.26"                # Excel 读取（xlsx/xls）
umya-spreadsheet = "2.3"       # Excel 读写（加载模板+写入指定单元格）
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
anyhow = "1"                     # 通用错误处理（占位，实际主要用 thiserror）
thiserror = "2"                  # derive 错误类型
tracing = "0.1"                  # 日志框架
tracing-subscriber = { version = "0.3", features = ["env-filter"] }  # 日志订阅器
tracing-appender = "0.2"         # 日志文件写入
chrono = "0.4"                   # 日期时间
regex = "1"                      # 正则（文本→数字、日期解析）
uuid = { version = "1", features = ["v4"] }  # 唯一标识
dirs = "6"                       # 系统目录

[dev-dependencies]
tempfile = "3"
zip = "2"

[profile.release]
strip = true; lto = true; codegen-units = 1; opt-level = "s"; panic = "abort"
```

### 前端（package.json）

| 依赖                                | 用途                       |
| ----------------------------------- | -------------------------- |
| react + react-dom ^18.3             | UI框架                     |
| react-router-dom ^6.28              | 路由                       |
| antd ^5.23 + @ant-design/icons ^5.5 | UI组件库                   |
| zustand ^5                          | 状态管理                   |
| dayjs ^1.11                         | 日期处理                   |
| @tauri-apps/api ^2.2 + plugins      | Tauri前端API               |
| typescript ^5.7                     | 类型检查                   |
| vite ^6 + @vitejs/plugin-react      | 构建工具                   |
| @playwright/test ^1.60              | E2E 测试（已安装，待配置） |

## 10. 测试

| 文件                        | 内容                                                                                         |
| --------------------------- | -------------------------------------------------------------------------------------------- |
| `tests/test_core.rs`        | 核心功能：数字解析（9个用例）、质量评分（6个用例）、日期工具（5个用例）、AI分析器（5个用例） |
| `tests/test_aggregation.rs` | 数据汇总：各引擎预览+执行                                                                    |
| `tests/test_xlsx_debug.rs`  | Excel 文件读取调试                                                                           |

运行：`cd src-tauri && cargo test`

## 11. 数据流

```
子公司 .xlsx → calamine(ExcelReader<RS>) → NumberParser(文本→数字)
   → AggregationEngine(4引擎) → HashMap<公司, HashMap<指标, 值>>
        ├── ReportWriter → 汇总 .xlsx（直接写入）
        └── AIAnalyzer → DeepSeek API → AnalysisQuality(4维评分，满分8)
              ├── score>=8 → 保存结果
              └── score<8 → 重试(≤2次（指数退避）)
                    └── ReportWriter → AI分析 Sheet + PPT文案
```

## 12. 权限配置（capabilities/default.json）

```json
{
    "identifier": "default",
    "windows": ["main"],
    "permissions": [
        "core:default",
        "dialog:default",
        "dialog:allow-open",
        "dialog:allow-save",
        "dialog:allow-ask",
        "dialog:allow-confirm",
        "dialog:allow-message",
        "fs:default",
        "fs:allow-read-text-file",
        "shell:default",
        "clipboard-manager:default",
        "clipboard-manager:allow-write-text",
        "clipboard-manager:allow-read-text"
    ]
}
```

## 13. 构建配置（tauri.conf.json）

```json
{
    "productName": "ExcelMiner",
    "version": "0.5.0",
    "identifier": "com.excelminer.app",
    "app": {
        "windows": [
            {
                "title": "ExcelMiner",
                "width": 1280,
                "height": 860,
                "minWidth": 1024,
                "minHeight": 700,
                "center": true,
                "resizable": true
            }
        ],
        "security": {
            "csp": "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com"
        }
    },
    "bundle": {
        "windows": {
            "wix": { "language": "zh-CN" },
            "nsis": { "languages": ["SimpChinese", "English"] }
        }
    }
}
```

## 14. 开发阶段

| 阶段    | 内容                                                           | 状态                 |
| ------- | -------------------------------------------------------------- | -------------------- |
| Phase 1 | Rust+Tauri框架、React+Antd前端、配置管理、错误/日志            | ✅                   |
| Phase 2 | 4汇总引擎、Excel读写、数字解析、YTD累计、日期工具              | ✅                   |
| Phase 3 | DeepSeek API客户端、提示词加载、逐公司分析+重试、4维度质量评分 | ✅                   |
| Phase 4 | xlsx报表写入、PPT文案导出、QualityChecker集成、页面联调        | ✅（主题样式待完善） |
| Phase 5 | 便携版打包、集成测试、MSI安装包                                | ✅                   |

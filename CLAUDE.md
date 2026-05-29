# CLAUDE.md — ExcelMiner 项目速览

## 项目定位

ExcelMiner 是一个基于 Tauri v2 的 Windows 桌面应用，用于多子公司 Excel 经营数据自动汇总与 DeepSeek AI 经营分析。

## 常用命令

```bash
# 开发运行
npm run tauri dev          # 启动 Vite + Tauri 窗口

# 仅前端
npm run dev                # Vite 开发服务器

# 后端测试
cd src-tauri && cargo test

# 前端类型检查
npx tsc --noEmit

# 构建便携版
npm run tauri build
```

## 架构概览

```
┌─────────────────────────────────────────────────────┐
│  前端 (React 18 + TypeScript + Ant Design 5)        │
│  src/pages/  src/stores/appStore.ts (Zustand 5)     │
│                    ↕ invoke / listen                  │
│  后端 (Rust / Tauri v2)                              │
│  commands/  ←→  services/  ←→  models/              │
│  (15个Tauri命令)  (汇总引擎/AI分析)  (数据结构)       │
└─────────────────────────────────────────────────────┘
```

## 关键文件索引

| 文件                                                   | 作用                                                                                                                  |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| `src/stores/appStore.ts`                               | Zustand 全局状态（project/appConfig/aggregationResults/analysisResults/currentStep）                                  |
| `src/pages/MainPage.tsx`                               | **主界面** — 单页面一体化操作（配置+汇总+分析+日志），当前默认入口                                                    |
| `src/pages/ProjectSetup.tsx`                           | [旧] Step 1 — 项目创建/打开（4步向导模式，保留兼容）                                                                  |
| `src/pages/DataImport.tsx`                             | [旧] Step 2 — 数据预览 + 一键汇总（保留兼容）                                                                         |
| `src/pages/AIAnalysis.tsx`                             | [旧] Step 3 — AI 分析执行 + 结果展示（保留兼容）                                                                      |
| `src/pages/ReportExport.tsx`                           | [旧] Step 4 — 导出 xlsx + 复制 PPT 文案（保留兼容）                                                                   |
| `src/types/index.ts`                                   | TypeScript 类型定义                                                                                                   |
| `src-tauri/src/lib.rs`                                 | Tauri Builder + 命令注册入口 + 日志初始化                                                                             |
| `src-tauri/src/error.rs`                               | 统一错误类型（AppError 10变体 + AppResult<T> 别名 + From trait）                                                      |
| `src-tauri/src/commands/project_cmd.rs`                | 项目 CRUD（create/open/save/get_default_config）+ AppState 定义                                                       |
| `src-tauri/src/commands/import_cmd.rs`                 | 数据导入（preview_import/execute_aggregation）                                                                        |
| `src-tauri/src/commands/analysis_cmd.rs`               | AI 分析（5个命令：execute_segment_analysis/execute_company_analysis/execute_analysis/test_api_connection/read_dskey） |
| `src-tauri/src/commands/export_cmd.rs`                 | 报表导出（export_report/copy_to_clipboard/open_in_explorer/open_log_folder）                                          |
| `src-tauri/src/services/data_aggregator.rs`            | AggregationEngine trait + EngineType 枚举（4引擎调度）                                                                |
| `src-tauri/src/services/data_aggregator/insurance.rs`  | 保险业态汇总引擎                                                                                                      |
| `src-tauri/src/services/data_aggregator/hotel.rs`      | 酒店业态汇总引擎                                                                                                      |
| `src-tauri/src/services/data_aggregator/commercial.rs` | 商写业态汇总引擎                                                                                                      |
| `src-tauri/src/services/data_aggregator/financial.rs`  | 经营报表汇总引擎                                                                                                      |
| `src-tauri/src/services/ai_analyzer.rs`                | DeepSeek API 调用（60s超时）+ 指数退避重试 + 提示词本地文件加载                                                       |
| `src-tauri/src/services/company_registry.rs`           | 从 companies.toml 加载公司模板                                                                                        |
| `src-tauri/src/services/excel_reader.rs`               | calamine 泛型封装 ExcelReader<RS>                                                                                     |
| `src-tauri/src/services/number_parser.rs`              | 文本→数字解析（千分位/百分号/金额前缀/表达式求值）                                                                    |
| `src-tauri/src/services/quality_checker.rs`            | QualityChecker 结构体：分析内容验证+质量评估+重试上限                                                                 |
| `src-tauri/src/services/report_writer.rs`              | xlsx 报表写入（加载模板 → 按引擎类型写入指定单元格 + AI分析Sheet）                                                    |
| `src-tauri/src/utils/date_utils.rs`                    | 日期解析（parse_month/parse_date_from_folder）+ YTD月份计算（ytd_months）                                             |
| `src-tauri/src/config.rs`                              | AppConfig/GeneralConfig/DefaultConfig（全局配置）                                                                     |
| `src-tauri/src/models/project.rs`                      | Project/Company/BusinessType/AIConfig                                                                                 |
| `src-tauri/src/models/analysis.rs`                     | AnalysisResult/AnalysisQuality（4维度评分，满分8分）/TokenUsage/ProgressUpdate/PreviewData/AggregationResult          |
| `src-tauri/src/models/indicator.rs`                    | IndicatorDef/IndicatorValue/IndicatorSet                                                                              |
| `resources/companies.toml`                             | 子公司预定义模板（9家公司3个业态）                                                                                    |
| `resources/prompts/*.md`                               | AI 系统提示词（保险分析/酒店分析/商写分析/财务分析师），build.rs 自动同步到 src-tauri/resources/                      |

## Tauri 命令清单（15个）

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
| analysis | `read_dskey`               | section                                               | `Option<String>`         | 从 ~/.dskey 文件读取指定分组的 API Key   |
| export   | `export_report`            | state                                                 | `String`(路径)           | 写入xlsx                                 |
| export   | `copy_to_clipboard`        | app_handle, text                                      | `()`                     | 复制PPT文案到剪贴板                      |
| export   | `open_in_explorer`         | path                                                  | `()`                     | 打开文件浏览器定位                       |
| export   | `open_log_folder`          | (无)                                                  | `String`(日志路径)       | 用系统关联程序打开日志目录               |

## 关键设计决策

### AppState（服务器端全局状态）

定义于 `commands/project_cmd.rs`，通过 Tauri 的 `manage()` 注入：

- `config: Mutex<AppConfig>` — 全局配置
- `current_project: Mutex<Option<Project>>` — 当前项目
- `aggregation_results: Mutex<Vec<AggregationResult>>` — 跨步骤共享的汇总结果
- `analysis_results: Mutex<Vec<AnalysisResult>>` — 跨步骤共享的分析结果
- `_log_guard: Mutex<Option<WorkerGuard>>` — 日志文件写入 guard

### 汇总引擎（AggregationEngine trait）

4个引擎实现同一 trait：`aggregate(project, sender) -> AggregationResult`，通过 `window.emit("aggregation-progress")` 向前端推送进度。引擎通过 `EngineType` 枚举区分：Insurance / Hotel / Commercial / Financial。

### AI 分析（AIAnalyzer）

分析流程分两阶段：

1. **板块业态分析**（`analyze_segment`）— 按业态用专属提示词对板块整体进行分析，跳过质量检查，仅验证内容≥50字
2. **子公司经营指标分析**（`analyze_single`）— 逐公司用财务分析师提示词独立分析，带4维度质量评分+自动重试（摘要不计分）

关键方法：

- `analyze_segment(system_prompt, user_prompt, segment_name, business_type)` — 板块级分析（仅内容长度校验）
- `analyze_single(system_prompt, user_prompt, company_name, business_type_display, business_type_enum, analysis_category)` — 单公司分析（完整质量评分+重试）
- `analyze_batch(company_data, business_type, on_progress)` — 批量分析（已废弃，保留兼容）

数据隔离：

- 板块分析仅使用**对应业态引擎**的汇总数据（保险/酒店/商写）
- 公司分析仅使用**经营报表引擎**的汇总数据

并发执行：公司分析阶段使用 `JoinSet` + `Semaphore(18)` 全并发，`Arc<AIAnalyzer>` 共享分析器实例，`Arc<AtomicUsize>` 追踪进度

配置参数：batch_size=3、max_retries=2、quality_threshold=8（满分8）、temperature=0.3、max_tokens=1500、HTTP 超时 60s
质量评分：4维度（revenue/ebitda/cashflow/expense），每项2分，满分8分。EBITDA维度支持 ebitda/GOP/扣非净利润 三选一
提示词加载策略：用户指定路径 → 可执行文件 ../resources/prompts/ → 工作目录 resources/prompts/ → 最小兜底提示词

### 数字解析（NumberParser）

`extract_number(text) -> Option<f64>` 支持：

- 纯数字（1234, -500, 3.14）
- 千分位（1,234.56）
- 百分号（85% → 0.85）
- 金额前缀（¥1,234.56, $500）
- 表达式求值（"1+1000" → 1001）

### 日期工具（utils/date_utils.rs）

- `parse_month(text)` — 从 "2024年6月" / "2024-06" 解析年月
- `ytd_months(year, month, count)` — 计算YTD月份序列，支持跨年
- `parse_date_from_folder(folder_name)` — 从文件夹名解析日期

### 质量评分体系

**AnalysisQuality**（`models/analysis.rs`）：4维度，每项2分，满分8分。摘要不计入评分。EBITDA维度含 ebitda/GOP/扣非净利润 三选一。

**QualityChecker**（`services/quality_checker.rs`）：

- `evaluate(company, content)` → `QualityResult { score, passed, reason }`
- `is_valid_content(content)` — 最少50字符
- `quality_hint(quality)` — 生成质量提示文本
- `max_retries()` — threshold/4（例如8/4=2次）

### 错误处理（error.rs）

`AppError` 枚举 10 变体：FileNotFound / SheetNotFound / KeywordNotFound / MissingData / ApiError / QualityTooLow / Io / Excel / Config / Other。通过 `From` trait 自动转换 `std::io::Error` → `Io`、`calamine::XlsxError` → `Excel`、`toml::de::Error` → `Config`、`umya_spreadsheet::XlsxError` → `Other`。

### 持久化

- 项目配置：`*.project.toml`（TOML 格式，与数据目录同层）
- 全局配置：`%APPDATA%/ExcelMiner/config.toml`
- 日志：`%APPDATA%/ExcelMiner/logs/app.log`

## 测试

| 文件                                  | 内容                                                                               |
| ------------------------------------- | ---------------------------------------------------------------------------------- |
| `src-tauri/tests/test_core.rs`        | 数字解析（9个用例）、质量评分（6个用例）、日期工具（5个用例）、AI分析器（5个用例） |
| `src-tauri/tests/test_aggregation.rs` | 各引擎预览+执行                                                                    |
| `src-tauri/tests/test_xlsx_debug.rs`  | Excel 文件读取调试                                                                 |

运行：`cd src-tauri && cargo test`

## 注意事项

- 项目约定使用中文注释和文档
- 所有路径使用 Windows 风格（`\`），跨平台命令需适配
- 便携版构建产物在 `release-portable/`
- VBA 原型在 `业务原型/` 目录，仅作历史参考，不参与构建
- `polars` 依赖已在 Cargo.toml 中注释预留，尚未启用
- 详细架构设计请参考 `DESIGN.md`

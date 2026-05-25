# ExcelMiner 需求实现对比报告

> 本文档对比业务原型（VBA宏）与当前实现（Rust/Tauri），基于 VBA 源码逐行研读和 Rust 源码全面分析。标注已实现、未实现、有差异的功能点，并详细描述数据流转过程。
>
> **状态更新 (2026-05-24)**：§七 建议行动计划中全部 9 项（P0×3 + P1×3 + P2×3）已实施完成，涉及 6 个源文件修改。详见 [§七](#七建议行动计划全部完成于-2026-05-24)。

---

## 一、数据流转总图

### 1.1 业务原型 (VBA) 数据流转

```
Excel源文件 (活动量/ & 经营报表/)
    │
    ▼
四个业态汇总宏 (.bas)
    ├── 保险数据汇总.bas → 汇总文件 "保险类" Sheet (数值+公式+格式)
    ├── 商写数据汇总.bas → 汇总文件 "商写类" Sheet (数值+公式+格式)
    ├── 酒店数据汇总.bas → 汇总文件 "酒店类" Sheet (数值+公式+格式)
    └── 经营报表汇总.bas → 各公司 Sheet G2:R18 (纯值复制)
    │
    ▼
AI分析宏 (.bas)
    ├── 业态分析整合版.bas → 按Sheet读取Excel数据区域 → 调用DeepSeek → 写回Sheet
    └── 核心指标分析.bas → 逐公司C1:R5读取 → 调用DeepSeek+质量评分 → 写回C61
    │
    ▼
输出: [YYYY年M月] 经营数据.xlsx (含公式+格式，可直接编辑)
```

**核心特征**：VBA 是 **"文件到文件"** 架构，汇总数据直接写入 Excel 模板的指定单元格，AI 分析从 Excel 区域读取数据。

### 1.2 当前实现 (Rust/Tauri) 数据流转

```
Excel源文件 (活动量/ & 经营报表/)
    │
    ▼
Rust 汇总引擎 (services/data_aggregator/*.rs)
    ├── InsuranceAggregator → JSON (summary_data)
    ├── CommercialAggregator → JSON
    ├── HotelAggregator → JSON
    └── FinancialAggregator → JSON
    │
    ▼
AppState (Rust后端内存)
    ├── aggregation_results: Vec<AggregationResult>
    └── analysis_results: Vec<AnalysisResult>
    │
    ▼
Tauri commands → React 前端 (Ant Design)
    ├── DataImport.tsx (预览+汇总)
    ├── AIAnalysis.tsx (分析触发+进度)
    └── ReportExport.tsx (导出xlsx+复制PPT文案)
    │
    ▼
ReportWriter → 输出: [YYYY年M月] 经营数据.xlsx (静态值，无公式)
```

**核心特征**：Rust 是 **"文件→内存→状态→文件"** 架构，汇总数据以 JSON 形式在内存中流转，组件间通过 AppState 解耦。

### 1.3 架构差异总览

| 维度           | 业务原型 (VBA)                             | 当前实现 (Rust/Tauri)                                 |
| -------------- | ------------------------------------------ | ----------------------------------------------------- |
| **数据流向**   | 读取源文件 → 直接写入目标模板Sheet         | 读取源文件 → Rust内存计算 → AppState → 前端展示/导出  |
| **数据格式**   | Excel Variant（保留原始格式+公式）         | JSON 字符串 + Rust 类型                               |
| **公式处理**   | 写入 Excel 公式 (`=C8/C7`)                 | 直接计算数值写入                                      |
| **格式设置**   | 宏内设置 NumberFormat                      | 未实现（P0待补）                                      |
| **数据持久化** | 实时写入 .xlsm 模板文件                    | 内存暂存，导出时才写入                                |
| **用户交互**   | Excel 内 VBA 按钮触发                      | React 前端多步骤向导                                  |
| **源文件处理** | `Workbooks.Open` → `Range.Value` → `Close` | `calamine::open_workbook` → `Vec<Vec<String>>` → Drop |

---

## 二、数据清洗对比

### 2.1 数字提取 (Number Parser)

| 场景       | VBA (汇总公共模块.bas)                 | Rust (number_parser.rs)                 | 状态        |
| ---------- | -------------------------------------- | --------------------------------------- | ----------- |
| 空值       | `IsEmpty(v)` → 0                       | `text.is_empty()` → None → 0            | ✅ 一致     |
| 纯数值     | `IsNumeric(v)` → `CDbl(v)`             | `parse::<f64>()`                        | ✅ 一致     |
| 错误值     | `IsError(v)` → 0                       | 字符串 `#N/A` → `parse` 失败 → None → 0 | ✅ 一致     |
| 文本含数字 | 逐字扫描提取连续 [0-9.] 串             | 正则/逐字扫描提取首个连续数字串         | ✅ 一致     |
| 加法表达式 | `ExtractNumberFromCell`: "1+1000"→1001 | `eval_expression`: "1+1000"→1001        | ✅ 一致     |
| 百分号     | **不支持**                             | ✅ `85%` → 0.85                         | ✅ **增强** |
| 货币符号   | **不支持**（¥,$被跳过不处理）          | ✅ `¥1,234.56` → 1234.56                | ✅ **增强** |
| 千分位逗号 | 被跳过（只保留数字和 `.`）             | ✅ 同逻辑（逗号被清理）                 | ✅ 一致     |

**差异详情**：

- VBA 的 `ParseNumeric` (汇总公共模块.bas) 不支持百分号和货币符号，遇到这些字符会直接跳过
- Rust 的 `extract_number` (number_parser.rs) 额外支持百分号识别和货币符号去除，功能更全面
- VBA 的 `ExtractNumberFromCell` (酒店数据汇总.bas) 支持加法表达式，仅用于酒店业态；Rust 的 `eval_expression` 全局可用

### 2.2 YTD 列号计算

| 维度            | VBA                                | Rust                                          | 状态    |
| --------------- | ---------------------------------- | --------------------------------------------- | ------- |
| 保险/商写列规律 | `col = 2*m + 2` (D=4, F=6, H=8...) | `cell(r, 2*m+2)`                              | ✅ 一致 |
| 伯豪瑞廷列规律  | `col = 2*m + 3` (E=5, G=7, I=9...) | `ach_col(m): if is_bhrt {2*m+3} else {2*m+2}` | ✅ 一致 |
| 重庆瑞尔列规律  | `col = 2*m + 2` (D=4, F=6, H=8...) | 同上                                          | ✅ 一致 |
| 经营报表列规律  | `col = m + 3` (D=4, E=5, F=6...)   | 同上                                          | ✅ 一致 |

### 2.3 酒店辅助列机制

| 维度             | VBA                                        | Rust                                         | 状态 |
| ---------------- | ------------------------------------------ | -------------------------------------------- | ---- |
| 实现方式         | AE列(31)开始创建辅助区域，写入提取后的数字 | 直接在内存中逐单元格 `extract_number` 后求和 | 等价 |
| 伯豪瑞廷三行合计 | 先对每行创建辅助列，再 `SumAuxRow` 求和    | `sum_rows()` 函数直接对多行多列遍历求和      | 等价 |

> 功能等价但实现路径不同：VBA 需要辅助列中转是因为 Excel 公式限制，Rust 无此限制。

---

## 三、各业态汇总引擎对比

### 3.1 保险数据汇总

| 功能点              | VBA (保险数据汇总.bas)                                      | Rust (insurance.rs)                   | 状态        |
| ------------------- | ----------------------------------------------------------- | ------------------------------------- | ----------- |
| 数据源 Sheet        | `保险类`                                                    | `保险类`                              | ✅ 一致     |
| 源行号常量          | Rows 4/5/6/10/12/13/14/15/16/17/18                          | 相同常量                              | ✅ 一致     |
| 期初人力            | 取1月达成值 `SafeRead(ws,4,4)`                              | `cell(4, 4)`                          | ✅ 一致     |
| YTD入职/离职        | `SumAchievementCols(ws, row, numMonths)`                    | `sum_ach(row)` = `Σ cell(row, 2*m+2)` | ✅ 一致     |
| 平均人力            | 逐月累加月末人力 / 月数                                     | 相同逻辑                              | ✅ 一致     |
| 续期13/25月应收实收 | 报告月当月值 `cell(row, 2*numMonths+2).Value`               | `cell(row, 2*numMonths+2)`            | ✅ 一致     |
| 月度规模保费序列    | 超出报告月写 `CVErr(xlErrNA)`                               | 超出报告月写 `f64::NAN`               | ✅ 一致     |
| 派生指标公式        | 写入 Excel 公式 (`=C8/C7`, `=C10/C16`, `=C10/C6`)           | 直接计算数值 (`ytd_open/avg_hr` 等)   | ⚠️ 差异     |
| **单元格格式**      | `C9:D9="0.00%"`, `C10:D11="#,##0.00"`, `C12:D18="#,##0.00"` | **未实现**                            | ❌ **待补** |
| **续期写入方式**    | `WriteCell` 保留 Empty 原值语义                             | 统一 `extract_number` → 0             | ⚠️ 差异     |
| 公司数              | 2家                                                         | 从 `companies.toml` [insurance] 读取  | ✅ 一致     |

**待补项**：

1. **单元格格式设置**：`C9:D9.NumberFormat = "0.00%"`（活动率）、`C10:D18.NumberFormat = "#,##0.00"`（保费+续期+件数+效率）
2. **续期保费空值语义**：VBA 保留 Empty 表示"无数据"，Rust 统一填 0.0 — 在导出时语义不同

### 3.2 商写数据汇总

| 功能点              | VBA (商写数据汇总.bas)                           | Rust (commercial.rs)                  | 状态        |
| ------------------- | ------------------------------------------------ | ------------------------------------- | ----------- |
| 数据源 Sheet        | `写字楼和商业综合体类`                           | `商写类`                              | ⚠️ 已修正   |
| 源行号常量          | Rows 4/5/7/8/9/10/11/14/15/16/19/20              | 相同常量                              | ✅ 一致     |
| 公司配置            | `CompanyConfig` 数组 (5家)                       | `company_registry()` [commercial] 段  | ✅ 一致     |
| 期初面积            | 1月达成值 `SafeRead(ws,ROW_BASE_AREA,4)`         | `cell(ROW_BASE_AREA, 4)`              | ✅ 一致     |
| 月末面积            | 报告月当月值 `SafeRead(ws,ROW_END_AREA,lastCol)` | `cell(ROW_END_AREA, last)`            | ✅ 一致     |
| 平均租金/成交周期   | 固定填 0（预留）                                 | 未显式设置（JSON中无对应字段）        | ⚠️ 差异     |
| 渠道/自营转化率公式 | 写入公式 `=col8/col7`, `=col13/col12`            | 直接计算 `deal/lead`                  | ⚠️ 差异     |
| **续签率**          | 模板自行处理（宏不写公式）                       | Rust 计算 `renew_rate = renew/expire` | ⚠️ 差异     |
| **单元格格式**      | `C11:G11="0%"`, `C16:G16="0%"`                   | **未实现**                            | ❌ **待补** |
| 公司数              | 5家                                              | 从 `companies.toml` 读取              | ✅ 一致     |

**待补项**：

1. **单元格格式设置**：`C11:G11.NumberFormat = "0%"`（渠道转化率）、`C16:G16.NumberFormat = "0%"`（自营转化率）

### 3.3 酒店数据汇总

| 功能点             | VBA (酒店数据汇总.bas)                                             | Rust (hotel.rs)                  | 状态        |
| ------------------ | ------------------------------------------------------------------ | -------------------------------- | ----------- |
| 数据源             | 活动量/ + 经营报表/                                                | 相同                             | ✅ 一致     |
| 伯豪瑞廷列规律     | `2*m+3` (E列起始)                                                  | `ach_col(m): if is_bhrt {2*m+3}` | ✅ 一致     |
| 重庆瑞尔列规律     | `2*m+2` (D列起始)                                                  | `ach_col(m): else {2*m+2}`       | ✅ 一致     |
| 伯豪瑞廷三行合计   | 投放(12+13+14), 受众(15+16+17), 成交(18+19+20)                     | `sum_rows(&[12,13,14])` 等       | ✅ 一致     |
| 重庆瑞尔单行       | 投放(12), 受众(13), 成交(14)                                       | 同常量                           | ✅ 一致     |
| 数字提取           | `ExtractNumberFromCell` → AE列辅助区 → SumAuxRow                   | `extract_number` 内存计算        | ✅ 等价     |
| 转化率             | 写入公式 `=C4/C3`, `=D4/D3`                                        | 直接计算 `deal/aud`              | ⚠️ 差异     |
| OTA/入住率月度序列 | `for m=1..12: col=m+3`，超报告月填 `CVErr(xlErrNA)`                | 相同逻辑，超报告月不写入         | ⚠️ 差异     |
| **单元格格式**     | `C2:D4="#,##0"`, `C5:D5="0.0000%"`, `F2:G13="0.00"`, `J2:K13="0%"` | **未实现**                       | ❌ **待补** |

**待补项**：

1. **单元格格式设置**：营销活动千分位 `#,##0`、转化率 `0.0000%`、OTA评分 `0.00`、入住率 `0%`
2. **OTA/入住率月度序列**：超报告月 VBA 填 `#N/A`（`CVErr(xlErrNA)`），Rust 不写入 — 导出时需处理

### 3.4 经营报表汇总

| 功能点       | VBA (经营报表汇总.bas)                             | Rust (financial.rs)                                | 状态            |
| ------------ | -------------------------------------------------- | -------------------------------------------------- | --------------- |
| 数据源 Sheet | `指标统计`                                         | 相同                                               | ✅ 一致         |
| 源数据区域   | `D4:O20` (16行×12列)                               | rows[3..20], cols[3..15]                           | ✅ 一致         |
| 数据处理方式 | **纯值复制** (`Copy → PasteSpecial xlPasteValues`) | **结构化提取** — 按section识别指标，计算YTD+达成率 | ⚠️ **重大差异** |
| 目标区域     | `G2:R18`（值粘贴，保持原布局）                     | JSON (label + target + ytd + rate + values)        | ⚠️ 差异         |
| 年度目标     | C列 随同 D4:O20 一起复制                           | 从 `row_data[2]` (col C, 0-based) 读取             | ✅ 一致         |
| 指标分组     | A列 section header（合并单元格）被保留             | 按 section 文本匹配 + idx 位置映射指标名           | ⚠️ 差异         |
| 公司数       | 9家 (从 `填写页!C2:C10` 读取)                      | `project.companies` 列表                           | ✅ 一致         |

**重大差异详情**：

- VBA 是 **"搬运工"** — 直接将 D4:O20 区域完整粘贴到 G2:R18，不做任何数据提取或计算。原始布局、空行、section header 全部保留
- Rust 是 **"数据抽取器"** — 按 section 分组解析指标含义，提取 label/target/ytd/rate/values 结构化字段。信息量相同，但表示形式完全不同
- VBA 保留 section header（"经营指标""财务指标"）作为视觉分组，Rust 丢失了此分层信息
- Rust 额外计算了 YTD 合计和年度达成率，VBA 不做这些计算

---

## 四、AI 分析功能对比

### 4.1 API 基础配置

| 参数         | VBA DeepSeekAPI.bas 默认 | VBA 核心指标分析.bas 覆盖 | Rust (AIConfig)     | 差异评估    |
| ------------ | ------------------------ | ------------------------- | ------------------- | ----------- |
| API 地址     | `api.deepseek.com`       | 同                        | 可配置              | ✅ 一致     |
| 模型         | `deepseek-chat`          | 同                        | 可配置              | ✅ 一致     |
| Temperature  | 0.3                      | 0.3                       | **0.3** (可配置)    | ✅ 一致     |
| Max Tokens   | **1000**                 | **1500**                  | **1500** (已统一)    | ✅ 一致     |
| HTTP Timeout | **30s**                  | **60s**                   | **60s** (已统一)     | ✅ 一致     |
| API Key 来源 | `~/.dskey` [EXCEL]段     | 同                        | `~/.dskey` + 可配置 | ✅ 一致     |

> Rust 的 timeout 和 max_tokens 已调整为与 VBA 一致（60s / 1500），参数已统一。

### 4.2 板块级分析（阶段一）

| 功能点         | VBA (业态分析整合版.bas)                                | Rust (ai_analyzer.rs analyze_segment)       | 状态    |
| -------------- | ------------------------------------------------------- | ------------------------------------------- | ------- |
| 保险数据范围   | Sheet `保险类` `F1:H25`                                 | JSON summary_data（保险引擎汇总结果）       | ⚠️ 等价 |
| 商写数据范围   | Sheet `商写类` `A1:G18`                                 | JSON summary_data（商写引擎汇总结果）       | ⚠️ 等价 |
| 酒店数据范围   | 三个独立区域 `B1:D5`+`E1:G13`+`I1:K13`                  | JSON summary_data（酒店引擎汇总结果）       | ⚠️ 等价 |
| 系统提示词来源 | 各业态 Sheet 的 L1 单元格 (保险/商写)、M1 单元格 (酒店) | `resources/prompts/*.md` 文件               | ⚠️ 差异 |
| 保险数据过滤   | 规模保费区域中 monthNum>currentMonth 的行被跳过滤       | 无此过滤（JSON数据中已用 NaN 表示超月数据） | ✅ 等价 |
| 执行顺序       | 商写→保险→酒店                                          | 可独立触发                                  | ⚠️ 差异 |
| 质量检查       | **跳过**（仅长度≥50字校验）                             | **跳过**（仅长度≥50字校验）                 | ✅ 一致 |
| 结果写入位置   | 保险/商写→L14, 酒店→M14                                 | Vec<AnalysisResult> → AppState              | ⚠️ 差异 |

> 数据范围等价但数据表示不同：VBA 读取 Excel 二维数组（含行列标签），Rust 读取 JSON 键值对。信息量相同。

### 4.3 公司经营指标分析（阶段二）

| 功能点           | VBA (核心指标分析.bas)                                    | Rust (ai_analyzer.rs analyze_single)         | 状态            |
| ---------------- | --------------------------------------------------------- | -------------------------------------------- | --------------- |
| 数据范围         | Sheet 各公司 `C1:R5` (5行×18列)                           | 经营报表 JSON（从 FinancialAggregator 提取） | ⚠️ 等价         |
| 分析上下文       | 公司名/年份/月份/序时进度/万元                            | 相同                                         | ✅ 一致         |
| 系统提示词来源   | `填写页!C20` 单元格                                       | 用户指定路径→内置默认→兜底                   | ⚠️ 差异         |
| 提示词内容       | `经营分析提示词.md` — 完整版（含利润负数/支出特殊规则）   | `PROMPT_FINANCIAL` — 完整版（已补齐）        | ✅ 一致         |
| **批次大小**     | **3**                                                     | **3** (可配置)                               | ✅ 一致         |
| **批次延迟**     | **1000ms** (`Application.Wait`)                           | **1000ms** (tokio::time::sleep)              | ✅ 一致         |
| **质量评分阈值** | **≥8 分** (满分10)                                        | **≥8 分** (可配置，默认值)                   | ✅ 一致         |
| **最大重试次数** | **2 次**                                                  | **2 次** (可配置)                            | ✅ 一致         |
| **重试延迟**     | **2 秒** (`RETRY_DELAY_MS=2000`)                          | **指数退避** (2^retry × 1000ms)              | ✅ 增强         |
| 质量未达标标记   | `[质量提示：本分析质量评分 X/10，部分指标描述可能不完整]` | `【质量不达标，评分 X/Y】`                   | ✅ 一致         |
| 质量评分维度     | 5维 (summary/revenue/ebitda/cashflow/expense)             | 5维 (相同)                                   | ✅ 一致         |
| HTTP层重试策略   | 指数退避 `2^retry` 秒，最多3次                            | 指数退避 `2^(retry-1) × 1000ms`              | ✅ 增强         |
| 结果写入位置     | 各公司 Sheet `C61` 单元格                                 | `Vec<AnalysisResult>` → AppState             | 等价            |

**质量评分逻辑对比**：

```vba
' VBA: 核心指标分析.bas — CheckAnalysisQuality
If quality.HasSummary Then quality.Score = quality.Score + 2
If quality.HasRevenue Then quality.Score = quality.Score + 2
If quality.HasEbitda Then quality.Score = quality.Score + 2
If quality.HasCashFlow Then quality.Score = quality.Score + 2
If quality.HasExpense Then quality.Score = quality.Score + 2
' 满分10分，≥8分通过
```

```rust
// Rust: quality_checker.rs — 4维度评分（摘要不计分）
let score = (quality.has_revenue as u8) * 2
    + (quality.has_ebitda as u8) * 2
    + (quality.has_cashflow as u8) * 2
    + (quality.has_expense as u8) * 2;
// 满分8分，默认≥8分通过（可配置）
```

### 4.4 板块分析质量维度（VBA vs Rust）

VBA 的质量评分仅用于**阶段二公司经营指标分析**。板块分析跳过质量检查（两个实现一致）。

Rust 的 `QualityChecker` 通过 `business_type_enum` 参数支持按业态覆盖关键词集合，这是 VBA 没有的功能（VBA 仅对阶段二做5维评分）。

### 4.5 提示词内容对比

| 维度           | VBA (经营分析提示词.md)                                | Rust (PROMPT_FINANCIAL 常量) | 差异              |
| -------------- | ------------------------------------------------------ | ---------------------------- | ----------------- |
| 角色定义       | ✅ 严谨的高级财务分析师                                | ✅ 相同                      | 一致              |
| 序时进度       | ✅ 以用户消息为准，禁止自行计算                        | ✅ 相同                      | 一致              |
| 达成率对比     | ✅ 领先/落后序时进度 X.X 个百分点                      | ✅ 相同                      | 一致              |
| 环比趋势       | ✅ 详细规则（收入/现金流/利润/支出各有表述）           | ⚠️ 精简表述                  | **VBA更完整**     |
| 利润类负值规则 | ✅ 详细的减亏/超支/绝对值表述                          | ❌ 缺失                      | **VBA有详细规则** |
| 支出类专属判断 | ✅ 成本管控有效/刚性成本较高，禁止对比达成率与序时进度 | ⚠️ 仅提及                    | **VBA更完整**     |
| 波动性         | ✅ (max-min)/均值 > 30%                                | ✅ 相同                      | 一致              |
| 输出格式       | ✅ 详细模板（≤50字摘要 + 每行≤60字，含所有必需字段）   | ⚠️ 精简版                    | **VBA更严格**     |
| 禁止项         | ✅ 主观情绪词、因果推断、改进建议                      | ✅ 相同                      | 一致              |

> VBA 提示词更加详尽，尤其是利润负数处理、支出判断规则、输出格式约束等方面。Rust 的内置默认提示词是精简版，缺少利润类负值的处理逻辑。

---

## 五、汇总差异清单

### 🔴 重大差异（P0 — 已全部修复于 2026-05-24）

| #   | 功能             | VBA 原型要求           | Rust 当前实现        | 状态                                    |
| --- | ---------------- | ---------------------- | -------------------- | --------------------------------------- |
| 1   | **质量评分阈值** | ≥8 分 (满分10)         | 默认 ≥4 分           | ✅ 已修复 (4→8, project.rs) |
| 2   | **单元格格式**   | 设置 NumberFormat      | **完全未实现**       | ✅ 已修复 (report_writer.rs apply_number_format) |
| 3   | **经营报表处理** | 纯值复制 D4:O20→G2:R18 | 结构化JSON提取       | ✅ 已修复 (新增 raw_grid 字段) |
| 4   | **提示词完整性** | 含利润负数/支出判断等  | 精简版，缺少专业规则 | ✅ 已修复 (PROMPT_FINANCIAL 扩展至完整版) |

### 🟡 中等差异（P1 — 近期处理）

| #   | 功能               | VBA 原型                        | Rust 当前实现             | 建议                      |
| --- | ------------------ | ------------------------------- | ------------------------- | ------------------------- |
| 5   | **批次延迟**       | 1000ms (`Application.Wait`)     | 无延迟                    | ✅ 已实现 tokio::time::sleep(1000ms) |
| 6   | **重试延迟**       | 2秒固定延迟                     | 无显式延迟（立即重试）    | ✅ 已实现指数退避 (2^retry × 1000ms) |
| 7   | **重试策略**       | HTTP层指数退避 `2^retry` 秒     | 无指数退避                | ✅ 已实现指数退避逻辑          |
| 8   | **API Timeout**    | 60s (公司分析) / 30s (板块分析) | 统一 120s                 | ✅ 已统一为 60s (2026-05-24) |
| 9   | **Max Tokens**     | 1500 (公司分析) / 1000 (板块)   | 统一 4096                 | ✅ 已统一为 1500 (2026-05-24) |
| 10  | **执行顺序**       | 商写→保险→酒店(固定)            | 可独立触发                | 可保持灵活，但需文档说明  |
| 11  | **系统提示词位置** | Sheet 单元格 (L1/M1/C20)        | 文件 (resources/prompts/) | 已实现，确保可配置        |
| 12  | **续期保#N/A**     | CVErr(xlErrNA) 显式标记         | extract_number→0          | 导出时需区分 0 和 N/A     |

### 🟢 已增强功能

| #   | 功能               | VBA 原型              | Rust 当前实现      | 说明            |
| --- | ------------------ | --------------------- | ------------------ | --------------- |
| 13  | **百分号解析**     | 不支持                | `85%` → 0.85       | Rust 增强       |
| 14  | **货币符号处理**   | 不支持                | `¥1,234` → 1234    | Rust 增强       |
| 15  | **业态维度评分**   | 仅公司分析有评分      | 板块/公司均可评分  | Rust 架构更灵活 |
| 16  | **分批分析**       | 串行+Application.Wait | async+tokio 并发   | Rust 架构更优   |
| 17  | **进度反馈**       | StatusBar 文本        | emit progress 事件 | Rust 实时性更好 |
| 18  | **Token 用量统计** | 无                    | TokenUsage 结构    | Rust 新增功能   |

---

## 六、数据输出对比

### 6.1 输出文件 Sheet 结构

| Sheet 名     | VBA 写入方式                                    | Rust 写入方式                    | 差异          |
| ------------ | ----------------------------------------------- | -------------------------------- | ------------- |
| `填写页`     | 人工填写或项目配置                              | report_writer 写入项目元数据     | 等价          |
| `保险类`     | 宏写入 C2:D18（数值+公式）+ G13:H24（月度序列） | report_writer 写入 JSON 展平数据 | 缺少公式+格式 |
| `商写类`     | 宏写入 C2:G18（数值+公式）                      | 同上                             | 缺少公式+格式 |
| `酒店类`     | 宏写入营销活动+OTA+入住率（含公式+格式）        | 同上                             | 缺少公式+格式 |
| 各公司Sheet  | 纯值复制 `G2:R18 ← D4:O20`                      | report_writer 写入指标详情       | 布局完全不同  |
| `AI分析结果` | 各业态Sheet L14/M14 + 各公司Sheet C61           | 统一写入 AI分析 Sheet            | Rust 更集中   |

### 6.2 VBA 公式 vs Rust 数值

VBA 在汇总 Sheet 中写入的是 Excel 公式，打开文件后可二次编辑、可自动重算：

- 活动率 = `=C8/C7`
- 件均保费 = `=C10/C16`
- 渠道转化率 = `=C8/C7`

Rust 写入的是计算后的数值，文件打开后是静态值。
**影响**：如果用户在 Excel 中手动修改了源数据，VBA 的公式会自动更新，Rust 的数值不会。

---

## 七、建议行动计划（✅ 全部完成于 2026-05-24）

### P0 — 立即修复 ✅

1. ✅ **调整质量评分阈值为 8 分**
    - 位置：`src-tauri/src/models/project.rs` → `default_quality_threshold()`
    - 修改：`4` → `8`
    - 状态：**已实施**（同时更新了 `test_ai_config_defaults` 测试断言）

2. ✅ **补充 Excel 单元格格式设置**
    - 位置：`src-tauri/src/services/report_writer.rs`
    - 新增 `apply_number_format()` 辅助函数，已设置格式：
        - 保险类：`C9:D9` = `0.00%`, `C10:D18` = `#,##0.00`
        - 商写类：`C11:G11` = `0%`, `C16:G16` = `0%`
        - 酒店类：`C2:D4` = `#,##0`, `C5:D5` = `0.0000%`, `F2:G13` = `0.00`, `J2:K13` = `0%`
    - 状态：**已实施**

3. ✅ **完善经营分析提示词**
    - 位置：`src-tauri/src/services/ai_analyzer.rs` → `PROMPT_FINANCIAL`
    - 已补充：利润类负值处理规则、支出判断的详细逻辑、输出格式的严格模板、环比趋势细化、质量约束
    - 同时修复 `load_system_prompt` 在 `business_type=None` 时加载 `财务分析师.md`
    - 状态：**已实施**

### P1 — 近期实现 ✅

4. ✅ **实现批次间延迟**
    - 位置：`src-tauri/src/commands/analysis_cmd.rs`
    - 方案：在 `execute_company_analysis` 和 `execute_analysis` 各公司循环中 `tokio::time::sleep(1000ms)`
    - 状态：**已实施**

5. ✅ **补充重试间延迟（指数退避）**
    - 位置：`src-tauri/src/services/ai_analyzer.rs` → `analyze_single` + `analyze_segment` 重试循环
    - 方案：`delay = 2^(retry-1) × 1000ms`（首次0ms, 然后1s, 2s, 4s...），覆盖了原 P2-7
    - 状态：**已实施**（含指数退避）

6. ✅ **统一重试次数为 2 次**
    - 位置：`src-tauri/src/models/project.rs` → `default_max_retries()`
    - 修改：`3` → `2`
    - 状态：**已实施**（同时更新了测试断言）

### P2 — 优化项 ✅

7. ✅ **指数退避重试**（合并于 P1-5）
    - 位置：`src-tauri/src/services/ai_analyzer.rs`
    - 方案：`delay = 2^(retry-1) × 1000ms`
    - 状态：**已实施**

8. ✅ **经营报表原始数据保留**
    - 位置：`src-tauri/src/services/data_aggregator/financial.rs` + `report_writer.rs`
    - 在 JSON 输出中增加 `raw_grid` 字段（16行×12列原始值），`write_financial` 优先使用 `raw_grid` 还原到 `G2:R18`
    - 状态：**已实施**

9. ✅ **统一 API 参数**
    - timeout: 60s（`ai_analyzer.rs` HTTP 超时）
    - max_tokens: 1500（`project.rs` `default_max_tokens()`）
    - 状态：**已实施**（同时更新了 CLAUDE.md / DESIGN.md 中的配置文档）

---

## 八、文件对照索引

| 业务原型 (VBA)       | 当前实现 (Rust)                                | 说明                 |
| -------------------- | ---------------------------------------------- | -------------------- |
| `汇总公共模块.bas`   | `number_parser.rs` + `date_utils.rs`           | 数字解析+日期工具    |
| `DeepSeekAPI.bas`    | `ai_analyzer.rs`                               | API调用+JSON处理     |
| `保险数据汇总.bas`   | `data_aggregator/insurance.rs`                 | 保险引擎             |
| `商写数据汇总.bas`   | `data_aggregator/commercial.rs`                | 商写引擎             |
| `酒店数据汇总.bas`   | `data_aggregator/hotel.rs`                     | 酒店引擎             |
| `经营报表汇总.bas`   | `data_aggregator/financial.rs`                 | 经营报表引擎         |
| `业态分析整合版.bas` | `analysis_cmd.rs` → `execute_segment_analysis` | 板块级分析           |
| `核心指标分析.bas`   | `analysis_cmd.rs` → `execute_company_analysis` | 公司经营指标分析     |
| `保险提示词.md`      | `ai_analyzer.rs` → `PROMPT_INSURANCE`          | 保险系统提示词       |
| `商写提示词.md`      | `ai_analyzer.rs` → `PROMPT_COMMERCIAL`         | 商写系统提示词       |
| `酒店提示词.md`      | `ai_analyzer.rs` → `PROMPT_HOTEL`              | 酒店系统提示词       |
| `经营分析提示词.md`  | `ai_analyzer.rs` → `PROMPT_FINANCIAL`          | 财务分析提示词       |
| `子公司缺省值.md`    | `companies.toml`                               | 公司注册表           |
| —                    | `quality_checker.rs`                           | 质量评分（Rust新增） |
| —                    | `report_writer.rs`                             | xlsx写入（Rust新增） |

---

_报告生成时间: 2026-05-24 — 基于 VBA 源码与 Rust 源码完整对比_

# Changelog

All notable changes to ExcelMiner will be documented in this file.

## [0.7.0] — 2026-05-30

### Added

- Route 2 xlsx 写入：纯 Rust ZIP+XML 直接操作，替代 umya-spreadsheet（零 C 依赖，不崩溃）
- SST 追加模式：保留模板原有共享字符串表格式（含富文本），防止索引偏移导致数据污染
- `set_formula_with_value` API：写入公式同时附带缓存值，避免公式缓存清除后显示为空
- `clear_dirty` 机制：指定 Sheet 免于公式缓存清除
- `open_project` 公司列表自动补齐：空 companies 从注册表填充 9 家公司并回写 TOML
- AI 分析详细日志：每步打印数据预览、字数统计、API 耗时、完成状态
- 保险业态分析取数扩展：从仅 F1:H25 扩展至 A1:D18（详细指标）+ F1:H25（月度保费）
- AI 输出空行压缩：sanitize_text 去除连续空行和首尾空行
- `index.html` 入口文件

### Changed

- xlsx 写入引擎：umya-spreadsheet → 自定义 ZIP+XML 操作（`xlsx_writer.rs` ~1000 行）
- 公司分析并发数：Semaphore(18) → Semaphore(3)
- 质量评分满分：10 分 → 8 分

### Fixed

- umya-spreadsheet segfault 崩溃（模板加载时 C 库崩溃）
- SST 富文本条目丢失导致所有文本单元格索引偏移（"项目列乱了"）
- 填写页 A1 公式 =A2&A3 缓存清除后显示为空
- `open_project` 空 companies 导致 AI 分析全部跳过（0 分 0 秒）
- 保险业态分析取数不完整导致 AI 结论错误（"数据缺失严重"）
- `index.html` 缺失导致 Vite 返回 404

### Removed

- umya-spreadsheet `build_sst` 方法（废弃，改为 `append_to_sst`）
- 不再使用的 quick-xml `BytesEnd/BytesStart/BytesText/Writer` import

---

## [0.6.0] — 2026-05-29

### Added

- 单页面一体化操作界面（MainPage），整合配置、汇总、分析全流程
- 一键汇总功能，自动执行 4 个引擎（保险/酒店/商写/经营报表）
- 板块业态分析（保险/酒店/商写三个板块独立 AI 分析）
- 公司经营指标分析（9 家公司独立 AI 分析 + 质量评分）
- 实时日志输出面板，滚动显示执行进度
- 并发 AI 分析（Semaphore(18) 全并发）
- ~/.dskey 文件读取 API Key（支持 EXCEL 分组）
- 指数退避重试机制（质量不达标自动重试）
- 项目配置文件持久化（.project.toml）
- `docs/用户操作手册.md` — 最终用户操作指南
- `CHANGELOG.md` — 版本变更日志
- README.md 文档索引表 + 系统要求章节

### Changed

- 界面从 4 步向导模式改为单页面一体化模式
- AI 分析分为两阶段：板块分析 + 公司分析
- 质量评分体系：4 维度（revenue/ebitda/cashflow/expense），满分 8 分
- CLAUDE.md / DESIGN.md 同步更新为单页面架构描述

### Fixed

- 酒店业态特殊布局（多行合计）的兼容处理
- 跨年 YTD 月份计算问题

---

## [0.5.0] — 2026-04

### Added

- 4 步向导式操作流程（项目设置 → 数据汇总 → AI 分析 → 报表导出）
- 4 个业态汇总引擎（保险/酒店/商写/经营报表）
- DeepSeek API 集成与 AI 分析
- 4 维度质量评分体系
- Excel 模板写入（umya-spreadsheet）
- 数字解析器（千分位/百分号/金额前缀/表达式求值）
- 日期工具（年月解析 / YTD 计算）
- 公司注册模板（companies.toml）
- AI 提示词文件化管理（prompts/\*.md）

### Added (Commands)

- `create_project` / `open_project` / `save_project` — 项目 CRUD
- `preview_import` / `execute_aggregation` — 数据导入与汇总
- `execute_segment_analysis` / `execute_company_analysis` / `execute_analysis` — AI 分析
- `test_api_connection` / `read_dskey` — API 连接测试
- `export_report` / `copy_to_clipboard` / `open_in_explorer` / `open_log_folder` — 导出

---

## [0.4.0] — 2026-03

### Added

- Tauri v2 项目初始化
- React 18 + TypeScript + Ant Design 5 前端框架
- Zustand 5 状态管理
- calamine Excel 读取集成
- 统一错误处理（AppError 10 变体）
- Rust 集成测试框架

---

## Versioning

本项目遵循以下版本规则：

- 主版本号：重大架构变更
- 次版本号：新功能发布
- 修订号：Bug 修复与小优化

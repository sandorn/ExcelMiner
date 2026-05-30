# ExcelMiner — 子公司经营数据汇总分析系统

基于 Tauri v2 的桌面应用，自动汇总多子公司 Excel 经营数据，调用 DeepSeek 大模型按业态生成经营分析报告。

**当前版本**: v0.8.0

## 技术栈

| 层          | 技术                                 |
| ----------- | ------------------------------------ |
| 桌面壳      | Tauri v2（Rust）                     |
| 前端        | React 18 + TypeScript + Ant Design 5 |
| 状态管理    | Zustand 5                            |
| Excel 读取  | calamine 0.26                        |
| Excel 写入  | 纯 Rust ZIP+XML（Route 2，~1100 行） |
| HTTP 客户端 | reqwest 0.12（rustls-tls）           |
| AI 模型     | DeepSeek Chat API                    |
| 构建产物    | `release-portable/` 便携版           |

## 功能概览

| 阶段        | 功能           | 说明                                                       |
| ----------- | -------------- | ---------------------------------------------------------- |
| 1. 数据汇总 | 4 引擎并行汇总 | 保险（2 家）/ 酒店（2 家）/ 商写（5 家）/ 经营报表（9 家），支持插件扩展 |
| 2. 板块分析 | 3 板块 AI 分析 | 商写 / 保险 / 酒店，带专属提示词和数据过滤                 |
| 3. 公司分析 | 9 公司 AI 分析 | 并发调用（可配置），质量评分 + 自动重试                    |
| 仪表盘      | 可视化分析     | KPI 卡片 + 趋势图 + 饼图 + 柱状图 + AI 摘要               |
| 导出        | 打开结果文件   | 三阶段完成后按钮激活，支持取消，定位 xlsx                  |

## 系统要求

| 项目     | 最低要求                |
| -------- | ----------------------- |
| 操作系统 | Windows 10 / Windows 11 |
| 内存     | 4 GB RAM                |
| 磁盘空间 | 200 MB 可用空间         |
| 网络     | 需连接 DeepSeek API     |

## 快速开始

```bash
npm install                # 安装依赖
npm run tauri dev          # 开发模式（Vite + Tauri）
npm run tauri build        # 构建便携版
```

## 测试

```bash
cd src-tauri && cargo test  # Rust 后端（88 个测试）
npx vitest run              # 前端（13 个测试）
npx tsc --noEmit            # TypeScript 类型检查
```

## 文档索引

| 文档                                 | 说明                         | 面向              |
| ------------------------------------ | ---------------------------- | ----------------- |
| [用户操作手册](docs/用户操作手册.md) | 完整操作指南、FAQ            | 最终用户          |
| [业务逻辑详解](docs/业务逻辑详解.md) | 汇总规则、指标计算、数据流转 | 业务人员 / 开发者 |
| [DESIGN.md](DESIGN.md)               | 系统架构设计                 | 开发者            |
| [GAP_ANALYSIS.md](GAP_ANALYSIS.md)   | VBA 原型 vs Rust 实现对比    | 开发者 / 维护者   |
| [CHANGELOG.md](CHANGELOG.md)         | 版本变更日志                 | 所有人            |
| [CLAUDE.md](CLAUDE.md)               | AI 助手指南（项目速览）      | AI 工具           |

## 许可

内部工具，未设定开源许可。

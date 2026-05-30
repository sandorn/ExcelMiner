# ExcelMiner — 子公司经营数据汇总分析系统

基于 Tauri v2 的桌面应用，自动汇总多子公司 Excel 经营数据，调用 DeepSeek 大模型按业态生成经营分析报告及 PPT 文案。

## 技术栈

| 层          | 技术                                 |
| ----------- | ------------------------------------ |
| 桌面壳      | Tauri v2（Rust）                     |
| 前端        | React 18 + TypeScript + Ant Design 5 |
| 状态管理    | Zustand 5                            |
| Excel 读取  | calamine 0.26                        |
| Excel 写入  | 纯 Rust ZIP+XML（Route 2）           |
| HTTP 客户端 | reqwest 0.12（rustls-tls）           |
| AI 模型     | DeepSeek Chat API                    |
| 构建产物    | `release-portable/` 便携版           |

## 系统要求

| 项目     | 最低要求                |
| -------- | ----------------------- |
| 操作系统 | Windows 10 / Windows 11 |
| 内存     | 4 GB RAM                |
| 磁盘空间 | 200 MB 可用空间         |
| 网络     | 需连接 DeepSeek API     |

## 快速开始

```bash
# 安装依赖
npm install

# 开发模式（Vite + Tauri）
npm run tauri dev

# 仅启动前端开发服务器
npm run dev

# 构建便携版
npm run tauri build
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

## 开发

```bash
# Rust 后端测试
cd src-tauri && cargo test

# 前端类型检查
npx tsc --noEmit
```

## 许可

内部工具，未设定开源许可。

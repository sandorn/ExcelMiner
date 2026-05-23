# ExcelMiner — 子公司经营数据汇总分析系统

基于 Tauri v2 的桌面应用，自动汇总多子公司 Excel 经营数据，调用 DeepSeek 大模型按业态生成经营分析报告及 PPT 文案。

## 技术栈

| 层          | 技术                                 |
| ----------- | ------------------------------------ |
| 桌面壳      | Tauri v2（Rust）                     |
| 前端        | React 18 + TypeScript + Ant Design 5 |
| 状态管理    | Zustand 5                            |
| Excel 读取  | calamine 0.26                        |
| Excel 写入  | umya-spreadsheet 2.3                 |
| HTTP 客户端 | reqwest 0.12（rustls-tls）           |
| AI 模型     | DeepSeek Chat API                    |
| 构建产物    | `ExcelMiner-v0.1-portable/` 便携版   |

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

## 项目结构

```
ExcelMiner/
├── src/                     # React 前端
│   ├── pages/               # 4个步骤页面（项目设置/数据汇总/AI分析/报表导出）
│   ├── stores/              # Zustand 状态管理
│   ├── styles/              # 全局样式
│   └── types/               # TypeScript 类型定义
├── src-tauri/               # Rust 后端
│   ├── src/
│   │   ├── commands/        # Tauri 命令（project/import/analysis/export，共15个）
│   │   ├── models/          # 数据模型（project/company/indicator/analysis）
│   │   ├── services/        # 业务逻辑（汇总引擎/AI分析/公司注册/Excel读写/报表写入）
│   │   └── utils/           # 工具函数（日期解析/YTD计算）
│   ├── Cargo.toml
│   └── tests/               # Rust 集成测试
├── resources/
│   ├── companies.toml       # 子公司预定义模板（9家公司3个业态）
│   └── prompts/             # AI 系统提示词（保险/酒店/商写/财务分析师）
├── 业务原型/                 # 原始 VBA 脚本与提示词参考（不参与构建）
├── DESIGN.md                # 架构设计文档（详见此文件）
└── CLAUDE.md                # 项目速览（AI 助手指南）
```

## 业务流程

1. **项目设置** — 输入项目名称、年月、数据目录，选择子公司和业态，配置AI参数
2. **数据汇总** — 选择业态引擎（保险/酒店/商写/经营报表），预览→一键汇总，实时进度显示
3. **AI 分析** — 配置 API Key + 提示词，逐公司生成经营分析，5维度质量评分（满分10）+ 自动重试（最多2次）
4. **报表导出** — 生成汇总 xlsx 文件（含汇总数据/AI分析/指标明细）+ PPT 文案复制到剪贴板

## 开发

```bash
# Rust 后端测试
cd src-tauri && cargo test

# 前端类型检查
npx tsc --noEmit
```

## 许可

内部工具，未设定开源许可。

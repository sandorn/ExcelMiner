# ExcelMiner 优化建议报告

> 本文档基于代码审查，提供架构、代码质量、性能、安全、可维护性等方面的优化建议。

---

## 一、架构层面

### 1.1 前后端状态同步机制存在风险

**现状**：

- Rust 后端使用 `AppState`（`Mutex<AggregationResult>`）存储跨步骤数据
- 前端使用 Zustand store 独立存储
- 两者通过 Tauri `invoke` 返回值手动同步

**问题**：

- 步骤回退时（如从 Step3 返回 Step2），后端状态可能被前端覆盖
- 页面刷新会导致前端状态丢失，而后端状态仍保留
- 状态不一致时难以调试

**建议**：

```typescript
// 方案A: 在 ProjectSetup 中检查并恢复后端状态
useEffect(() => {
    invoke<Project>('get_current_project').then((p) => {
        if (p) setProject(p);
    });
}, []);

// 方案B: 使用 Tauri 持久化存储插件 (tauri-plugin-store)
// 将状态序列化到本地文件，刷新后自动恢复
```

### 1.2 硬编码行列号常量分散在多个引擎中

**现状**：

- `insurance.rs` 定义了 `ROW_BASE_HR = 4`、`ROW_HR_IN = 5` 等常量
- `hotel.rs`、`commercial.rs` 各自重复定义类似常量
- 无统一的数据结构定义

**建议**：

```rust
// 创建统一的指标定义模块
pub mod indicator_schema {
    /// 指标定义：行号、列规律、是否YTD累加
    pub struct IndicatorDef {
        pub name: &'static str,
        pub row: usize,
        pub col_pattern: ColPattern,
        pub aggregate: AggregateType,
    }

    pub enum ColPattern {
        Fixed(usize),           // 固定列
        MonthBased(usize),      // 2*m + offset
    }

    pub enum AggregateType {
        None,                   // 不累加，取当月值
        YtdSum,                 // YTD求和
        YtdAverage,             // YTD平均
    }

    // 各业态的指标schema集中管理
    pub const INSURANCE_INDICATORS: &[IndicatorDef] = &[
        IndicatorDef { name: "期初人力", row: 4, col_pattern: Fixed(4), aggregate: None },
        // ...
    ];
}
```

### 1.3 Trait Object 动态分发带来的性能开销

**现状**：

```rust
pub trait AggregationEngine: Send + Sync {
    fn engine_type(&self) -> EngineType;
    fn execute(&self, project: &Project) -> AppResult<AggregationResult>;
}

// 使用时通过 Box<dyn AggregationEngine> 调用
```

**问题**：

- 每个引擎实现都需要堆分配 + 虚函数调用
- 4个引擎是固定组合，动态分发收益有限

**建议**：

```rust
// 方案A: 使用枚举替代 trait object
pub enum AggregationEngines {
    Insurance(InsuranceAggregator),
    Hotel(HotelAggregator),
    Commercial(CommercialAggregator),
    Financial(FinancialAggregator),
}

impl AggregationEngines {
    pub fn execute(&self, project: &Project) -> AppResult<AggregationResult> {
        match self {
            Self::Insurance(e) => e.execute(project),
            Self::Hotel(e) => e.execute(project),
            // ...
        }
    }
}

// 方案B: 若确实需要灵活性，使用函数指针而非 trait object
pub type ExecuteFn = fn(&Project) -> AppResult<AggregationResult>;
```

---

## 二、代码质量

### 2.1 `anyhow` 依赖未被充分利用

**现状**：

```toml
# Cargo.toml
anyhow = "1"
```

但代码中主要使用 `thiserror` 定义的 `AppError`，`anyhow` 仅为占位注释：

```rust
// anyhow = "1"  // 通用错误处理（占位，实际主要用 thiserror）
```

**建议**：移除 `anyhow` 依赖，或将部分内部错误转换逻辑改用 `anyhow::Context` 简化

### 2.2 魔法数字分散且无注释

**现状**：

```rust
// insurance.rs
let last_col = 2*num_months+2;  // 什么含义？
// 源数据中，达成列规律：第 m 月的达成列 = 2×m + 2
```

**建议**：

```rust
/// 源数据列规律：第 m 月的"达成"列号 = 2*m + 2
/// 例如：1月达成=D列(列号4), 2月达成=F列(列号6), ...
/// @see 业务原型/业务逻辑详解.md §3.1
const fn achieve_col(month: usize) -> usize { 2 * month + 2 }

let last_col = achieve_col(num_months);
```

### 2.3 错误处理模式不统一

**现状**：

```rust
// ai_analyzer.rs
if !response.status().is_success() {
    return Err(AppError::ApiError { ... });
}

// 有些地方直接 unwrap
let content = chat_response.choices.first()
    .map(|c| c.message.content.clone())
    .unwrap_or_default();  // 空响应被视为成功
```

**建议**：

```rust
// 统一错误处理策略
let content = chat_response.choices.first()
    .ok_or_else(|| AppError::ApiError {
        retry: 0,
        message: "API返回空choices".into()
    })?
    .message.content.clone();
```

### 2.4 缺少日志脱敏处理

**现状**：

```rust
tracing::info!(
    "[经营分析] {}: system={}chars user={}chars\n---user_prompt---\n{}\n---end---",
    company_name, system_prompt.len(), user_prompt.len(), user_prompt
);
```

**问题**：

- 日志中可能包含公司财务数据
- API Key 通过 header 传递，虽不在日志中，但应明确标注

**建议**：

```rust
// 添加日志脱敏开关
#[cfg(feature = "sensitive-logging")]
fn log_user_prompt(prompt: &str) -> String {
    // 生产环境仅记录长度和摘要
    format!("[长度:{}] {}", prompt.len(), &prompt[..prompt.len().min(100)])
}

#[cfg(not(feature = "sensitive-logging"))]
fn log_user_prompt(prompt: &str) -> String {
    prompt.to_string()
}
```

---

## 三、性能优化

### 3.1 串行文件读取可改为并行

**现状**：

```rust
// insurance.rs execute()
for company in &registry.insurance {
    let mut reader = match ExcelReader::open(&path) { ... };
    let data = match reader.read_sheet("保险类") { ... };
    // 串行处理每家公司
}
```

**问题**：

- 9家公司串行读取，若每家1秒，总计9秒
- CPU多核未被利用

**建议**：

```rust
use tokio::task::JoinSet;

pub async fn execute_async(&self, project: &Project) -> AppResult<AggregationResult> {
    let mut set = JoinSet::new();

    for company in &registry.insurance {
        let folder = project.data_folder.join("活动量");
        let path = folder.join(format!("{}.xlsx", company.name));

        set.spawn(async move {
            process_company(&path, &company.name).await
        });
    }

    let mut results = Vec::new();
    while let Some(res) = set.join_next().await {
        if let Ok(Ok(data)) = res {
            results.push(data);
        }
    }
    // ...
}
```

### 3.2 预览阶段重复读取文件

**现状**：

- `preview()` 打开文件检测结构
- `execute()` 再次打开同一文件读取数据
- 同一文件被打开两次

**建议**：

```rust
// 返回文件元数据 + 数据内容，避免重复IO
pub struct PreviewResult {
    pub path: PathBuf,
    pub sheets: Vec<String>,
    pub sample_data: Vec<Vec<String>>,  // 前10行预览
}

pub struct ExecuteResult {
    pub preview: PreviewResult,
    pub full_data: Vec<serde_json::Value>,
}

// 或使用缓存
use std::collections::HashMap;
use std::sync::Mutex;

struct FileCache(Mutex<HashMap<PathBuf, ExcelData>>);

impl FileCache {
    fn get_or_load(&self, path: &Path) -> AppResult<ExcelData> {
        let mut cache = self.0.lock().unwrap();
        if let Some(data) = cache.get(path) {
            return Ok(data.clone());
        }
        let data = ExcelReader::open(path)?.read_all()?;
        cache.insert(path.to_path_buf(), data.clone());
        Ok(data)
    }
}
```

### 3.3 JSON序列化开销

**现状**：

```rust
// analysis.rs
pub struct AnalysisResult {
    pub summary_data: String,  // JSON字符串
}
```

**问题**：

- `serde_json::to_string()` 每次聚合结果都要序列化
- 前端收到后再 `JSON.parse()` 解析回对象

**建议**：

```rust
// 直接传递结构化数据
#[derive(Serialize, Deserialize)]
pub struct AggregationResult {
    // 移除 String 类型，改用结构化数据
    pub companies: Vec<CompanySummary>,
}

#[derive(Serialize, Deserialize)]
pub struct CompanySummary {
    pub name: String,
    pub indicators: HashMap<String, f64>,
    pub monthly_series: Vec<f64>,  // 12个月数据
}

// 前端直接使用，无需二次解析
```

---

## 四、安全建议

### 4.1 API Key 存储方式

**现状**：

```rust
// 用户在UI中输入API Key，存储在项目配置中
api_key = "sk-xxxxx"  // 明文写入 .project.toml
```

**问题**：

- API Key 明文存储在配置文件
- 配置文件可能意外提交到代码仓库

**建议**：

```toml
# .gitignore
*.project.toml

# 使用系统密钥链存储
# Windows: Windows Credential Manager
# macOS: Keychain
# Linux: libsecret
```

### 4.2 文件路径验证缺失

**现状**：

```rust
let path = folder.join(format!("{}.xlsx", company_name));
// company_name 来自 TOML 配置，可能包含 "../" 等路径遍历字符
```

**建议**：

```rust
use std::path::Path;

fn sanitize_filename(name: &str) -> AppResult<String> {
    // 移除路径分隔符和特殊字符
    let sanitized: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || c.is_whitespace())
        .collect();

    if sanitized.is_empty() {
        return Err(AppError::Config("公司名称无效".into()));
    }
    Ok(sanitized)
}

let filename = sanitize_filename(&company.name)?;
let path = folder.join(format!("{}.xlsx", filename));
```

### 4.3 外部程序执行

**现状**：

```rust
// export_cmd.rs
Shell::open(&path)  // 打开文件浏览器
```

**建议**：

- 限制可打开的目录范围
- 添加用户确认弹窗

---

## 五、可维护性

### 5.1 测试覆盖不足

**现状**：

- `tests/test_core.rs` 包含基础单元测试
- 无前端 E2E 测试（Playwright 已安装但未配置）
- 无集成测试验证完整数据流

**建议**：

```typescript
// e2e/basic.spec.ts
import { test, expect } from '@playwright/test';

test('完整工作流', async ({ page }) => {
    // Step 1: 创建项目
    await page.goto('/setup');
    await page.fill('[data-testid="project-name"]', '测试项目');
    await page.click('text=下一步');

    // Step 2: 执行汇总
    await page.click('text=保险数据汇总');
    await page.click('text=一键汇总');
    await expect(page.locator('.ant-alert')).toContainText('汇总成功');

    // Step 3: 执行分析
    await page.click('text=下一步');
    await page.fill('[data-testid="api-key"]', process.env.API_KEY);
    // ...
});
```

### 5.2 提示词硬编码部分较长

**现状**：
`default_prompt_for()` 函数返回的内置提示词是长字符串

**建议**：

- 将提示词提取到独立 `.md` 文件
- 提供打包时的默认值嵌入机制

```rust
// prompts.rs
include_str!("../../../resources/prompts/保险分析.md")

// 或使用 rust-embed 在二进制中嵌入
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../resources/prompts"]
struct Prompts;

const INSURANCE_PROMPT: &str = Prompts::get("保险分析.md").unwrap();
```

### 5.3 文档与代码不同步

**现状**：

- `业务原型/业务逻辑详解.md` 描述的业务逻辑
- 代码中的实现可能存在细微差异
- 无自动化方式验证一致性

**建议**：

```rust
// 在代码中添加注释引用文档
/// 保险汇总指标定义
///
/// 业务逻辑参考: docs/业务逻辑详解.md §3.1
/// 实现与文档一致性: tests/test_aggregation_alignment.rs
const INSURANCE_METRICS: &[IndicatorDef] = &[
    // ...
];
```

---

## 六、用户体验优化

### 6.1 进度反馈不精细

**现状**：

```rust
// 进度更新仅在每批开始时触发
on_progress(ProgressUpdate {
    step: format!("正在分析 {}业态 → {} (第{}/{}批)", ...),
    progress: (batch_idx as f64) / (total_batches as f64),
    // ...
});
```

**建议**：

```rust
// 实时显示 Token 使用量和预估完成时间
on_progress(ProgressUpdate {
    step: format!("正在分析 {} (第{}/{}批)", company_name, idx, total),
    progress: current_progress,
    status: Running,
    company: Some(company_name.clone()),
    // 新增字段
    token_used: usage.as_ref().map(|u| u.total_tokens),
    estimated_remaining: estimated_seconds,
});
```

### 6.2 错误恢复体验差

**现状**：

- API调用失败后自动重试，用户看不到中间状态
- 重试耗尽后仅返回错误信息，无恢复选项

**建议**：

```typescript
// 前端增加"重试"和"跳过"选项
const handleAnalysisError = (result: AnalysisResult) => {
    if (!result.success) {
        Modal.confirm({
            title: '分析失败',
            content: <>
                <p>{result.error_message}</p>
                <p>是否重试此公司？</p>
            </>,
            okText: '重试',
            cancelText: '跳过',
            onOk: () => invoke('retry_company_analysis', { company: result.company_name }),
            onCancel: () => invoke('skip_company'),
        });
    }
};
```

### 6.3 快捷键支持

**现状**：无键盘快捷键

**建议**：

- `Ctrl+Enter`: 执行当前步骤
- `Ctrl+S`: 保存项目
- `Escape`: 取消正在执行的操作

---

## 七、配置管理

### 7.1 配置热重载缺失

**现状**：

- 启动时加载 `config.toml`
- 修改配置需重启应用

**建议**：

```rust
// 使用 tauri-plugin-store 监听配置变化
const store = await Store.load('config.json');
store.onKeyChange('api_url', (newValue) => {
    // 更新运行时配置
    updateApiConfig({ api_url: newValue });
});
```

### 7.2 缺乏配置验证

**现状**：

```rust
// 项目配置可能包含无效值
pub struct Project {
    pub year: u32,
    pub month: u32,  // 未校验 1-12
    pub ytd_months: u32,  // 未校验 1-12
}
```

**建议**：

```rust
impl Project {
    pub fn validate(&self) -> AppResult<()> {
        if !(1..=9999).contains(&self.year) {
            return Err(AppError::Config("年份无效".into()));
        }
        if !(1..=12).contains(&self.month) {
            return Err(AppError::Config("月份必须在1-12之间".into()));
        }
        if !(1..=12).contains(&self.ytd_months) {
            return Err(AppError::Config("YTD月份必须在1-12之间".into()));
        }
        Ok(())
    }
}
```

---

## 八、技术债务

### 8.1 TODO/FIXME 注释

建议搜索并处理：

```bash
grep -r "TODO\|FIXME\|XXX\|HACK" src-tauri/src/
```

### 8.2 未使用的导出

```rust
// src-tauri/src/models/company.rs
pub use super::project::Company;  // 重导出用途不明
```

### 8.3 版本锁定过于严格

```toml
# package.json
"antd": "^5.23.0"  # 使用 ^ 允许补丁版本更新

# Cargo.toml
calamine = "0.26"  # 精确版本，不允许补丁更新
```

建议统一使用 `~` 或精确版本策略

---

## 九、优先级建议

| 优先级 | 建议项           | 影响         | 工作量 |
| ------ | ---------------- | ------------ | ------ |
| P0     | 状态同步机制修复 | 防止数据丢失 | 中     |
| P0     | API Key 安全存储 | 安全性       | 高     |
| P1     | 配置验证         | 稳定性       | 低     |
| P1     | 并行文件读取     | 性能         | 中     |
| P1     | 单元测试补充     | 可维护性     | 中     |
| P2     | 日志脱敏         | 安全性       | 低     |
| P2     | 错误恢复UI       | 用户体验     | 中     |
| P2     | E2E测试          | 可维护性     | 高     |
| P3     | 指标schema重构   | 可维护性     | 高     |
| P3     | 快捷键支持       | 用户体验     | 低     |

---

## 十、总结

ExcelMiner 项目整体架构清晰，采用 Tauri 实现桌面应用是一个明智的选择。核心业务逻辑（数据汇总、AI分析）与 V1 VBA 版本对应良好。

主要改进方向：

1. **稳定性**：修复状态同步和配置验证问题
2. **性能**：并行化 IO 操作，减少重复计算
3. **安全**：保护 API Key，增强路径验证
4. **可维护性**：补充测试，统一代码风格

建议按优先级逐步推进，避免大范围重构影响现有功能。

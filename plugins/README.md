# ExcelMiner 插件开发示例

## 目录结构

```
plugins/
  sample_plugin/
    Cargo.toml           # 插件 crate 配置
    src/
      lib.rs             # 实现 EnginePlugin trait + create_engine 导出
  README.md              # 本文件
```

## 快速开始

### 1. 创建插件 crate

```bash
cargo new --lib plugins/my_plugin
```

### 2. 配置 Cargo.toml

```toml
[package]
name = "my_plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
# 引用 excelminer_lib 获取 EnginePlugin trait 和模型类型
excelminer_lib = { path = "../../src-tauri" }
```

### 3. 实现插件

```rust
// plugins/my_plugin/src/lib.rs
use excelminer_lib::services::engine_plugin::EnginePlugin;
use excelminer_lib::models::analysis::{AggregationResult, PreviewData};
use excelminer_lib::models::project::Project;
use excelminer_lib::error::AppResult;

struct MyPlugin;

impl EnginePlugin for MyPlugin {
    fn plugin_id(&self) -> &str { "my_plugin" }
    fn display_name(&self) -> &str { "我的自定义汇总" }

    fn preview(&self, project: &Project) -> AppResult<PreviewData> {
        Ok(PreviewData {
            engine_name: self.display_name().into(),
            files_found: vec![],
            sheets_detected: vec![],
            companies_detected: vec![],
            available_indicators: vec![],
            warnings: vec!["示例插件：请在 data_folder 下放置数据文件".into()],
        })
    }

    fn execute(&self, project: &Project) -> AppResult<AggregationResult> {
        Ok(AggregationResult {
            engine_name: self.display_name().into(),
            companies_processed: 0,
            indicators_collected: 0,
            summary_data: String::new(),
            warnings: vec!["示例插件：请实现实际汇总逻辑".into()],
        })
    }
}

/// 插件入口：必须导出此符号
#[no_mangle]
pub extern "C" fn create_engine() -> *mut dyn EnginePlugin {
    Box::into_raw(Box::new(MyPlugin))
}
```

### 4. 构建并部署

```bash
cd plugins/my_plugin
cargo build --release
copy target\release\my_plugin.dll ..\      # 复制到 plugins/ 目录
```

重启 ExcelMiner 即可自动发现并加载新插件。

## 可用类型

插件可引用的公共类型（从 `excelminer_lib` 导入）：

| 路径                                      | 说明                                  |
| ----------------------------------------- | ------------------------------------- |
| `services::engine_plugin::EnginePlugin`   | 插件必须实现的 trait                  |
| `models::project::Project`                | 项目配置（data_folder, companies 等） |
| `models::analysis::AggregationResult`     | 汇总结果                              |
| `models::analysis::PreviewData`           | 预览数据                              |
| `error::AppResult`                        | `Result<T, AppError>` 别名            |
| `services::excel_reader::ExcelReader`     | Excel 文件读取器                      |
| `services::number_parser::extract_number` | 数字解析工具                          |

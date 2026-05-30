//! 插件化汇总引擎 — 运行时动态加载
//!
//! ## 架构
//!
//! ```text
//! plugins/                          ← 放置 .dll 文件的目录
//!   retail_plugin.dll               ← 零售业态引擎
//!   property_plugin.dll             ← 物业业态引擎
//!
//! src-tauri/src/services/
//!   engine_plugin.rs                ← 本文件：EnginePlugin trait + EngineRegistry
//!   data_aggregator.rs              ← 静态分发层：内置引擎 + 插件调度
//! ```
//!
//! ## 插件开发规范
//!
//! 1. 创建独立的 Rust `cdylib` crate
//! 2. 依赖 `excelminer_lib`（通过 path 或 git）
//! 3. 实现 `EnginePlugin` trait
//! 4. 导出 `#[no_mangle] pub extern "C" fn create_engine() -> *mut dyn EnginePlugin`
//!
//! 参见 `plugins/sample_plugin/` 示例。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{AppError, AppResult};
use crate::models::analysis::{AggregationResult, PreviewData};
use crate::models::project::Project;

// ── 插件接口 ────────────────────────────────────────────────────────────

/// 插件化汇总引擎 Trait（动态加载用）
///
/// 每个 `.dll` 插件必须实现此 trait 并通过 `create_engine()` 导出。
/// 与内置 `AggregationEngine` trait 保持相同语义。
pub trait EnginePlugin: Send + Sync {
    /// 引擎唯一标识（如 "retail"、"property"）
    fn plugin_id(&self) -> &str;

    /// 引擎显示名称（如 "零售数据汇总"）
    fn display_name(&self) -> &str;

    /// 预览：扫描文件，发现数据源
    fn preview(&self, project: &Project) -> AppResult<PreviewData>;

    /// 执行汇总，返回结构化结果
    fn execute(&self, project: &Project) -> AppResult<AggregationResult>;
}

// ── 插件注册表 ──────────────────────────────────────────────────────────

/// 加载的插件实例（持有动态库和引擎指针的双重所有权）
struct PluginInstance {
    /// 引擎指针（Arc 包装，支持跨任务克隆）
    engine: Arc<dyn EnginePlugin>,
    /// 动态库句柄（必须在此 struct 被 drop 前保持存活）
    _library: libloading::Library,
}

// 显式声明 Send + Sync（libloading::Library 是 Send 的）
unsafe impl Send for PluginInstance {}
unsafe impl Sync for PluginInstance {}

/// 引擎注册表：管理内置引擎 + 动态加载的插件引擎
pub struct EngineRegistry {
    /// 内置引擎（编译时已知，无需动态加载）
    builtin: HashMap<String, Arc<dyn EnginePlugin>>,
    /// 动态加载的插件
    plugins: Vec<PluginInstance>,
    /// 插件搜索目录
    plugin_dir: PathBuf,
}

impl EngineRegistry {
    /// 创建空的注册表
    pub fn new() -> Self {
        let plugin_dir = Self::default_plugin_dir();
        Self {
            builtin: HashMap::new(),
            plugins: Vec::new(),
            plugin_dir,
        }
    }

    /// 默认插件目录：可执行文件旁的 plugins/
    fn default_plugin_dir() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("plugins")))
            .unwrap_or_else(|| PathBuf::from("plugins"))
    }

    /// 注册一个内置引擎（编译时静态链接）
    pub fn register_builtin(&mut self, engine: Box<dyn EnginePlugin>) {
        let id = engine.plugin_id().to_string();
        tracing::info!("[EngineRegistry] 注册内置引擎: {}", id);
        self.builtin.insert(id, Arc::from(engine));
    }

    /// 从 plugins/ 目录加载所有 .dll 文件
    ///
    /// 加载失败不阻塞启动，仅记录警告。
    pub fn discover_plugins(&mut self) {
        if !self.plugin_dir.exists() {
            tracing::info!(
                "[EngineRegistry] 插件目录不存在: {}",
                self.plugin_dir.display()
            );
            return;
        }

        let entries = match std::fs::read_dir(&self.plugin_dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("[EngineRegistry] 无法读取插件目录: {}", e);
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "dll" && ext != "so" && ext != "dylib" {
                continue;
            }

            match self.load_plugin(&path) {
                Ok(()) => {
                    tracing::info!(
                        "[EngineRegistry] ✅ 加载插件: {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "[EngineRegistry] ⚠ 加载插件失败 {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
    }

    /// 加载单个 .dll 插件文件
    fn load_plugin(&mut self, path: &Path) -> AppResult<()> {
        // SAFETY: libloading 的安全性取决于插件 DLL 是否可信。
        // 仅从应用目录下的 plugins/ 加载，降低风险。
        unsafe {
            let library =
                libloading::Library::new(path).map_err(|e| AppError::Other(format!(
                    "无法加载动态库 {}: {}",
                    path.display(),
                    e
                )))?;

            // 查找 create_engine 符号
            let create_engine: libloading::Symbol<
                unsafe extern "C" fn() -> *mut dyn EnginePlugin,
            > = library
                .get(b"create_engine")
                .map_err(|e| AppError::Other(format!(
                    "插件 {} 缺少 create_engine 导出: {}",
                    path.display(),
                    e
                )))?;

            // 调用工厂函数
            let raw_ptr = create_engine();
            if raw_ptr.is_null() {
                return Err(AppError::Other(format!(
                    "插件 {} create_engine 返回空指针",
                    path.display()
                )));
            }

            let engine = Box::from_raw(raw_ptr);
            let id = engine.plugin_id().to_string();

            // 检查是否与已有引擎 ID 冲突
            if self.builtin.contains_key(&id) {
                tracing::warn!(
                    "[EngineRegistry] 插件 '{}' 与内置引擎 ID 冲突，跳过",
                    id
                );
                return Ok(());
            }

            self.plugins.push(PluginInstance {
                engine: Arc::from(engine),
                _library: library,
            });

            tracing::info!("[EngineRegistry] 已注册插件: {} ({})", id, path.display());
            Ok(())
        }
    }

    /// 根据引擎 key 查找引擎（先查内置，再查插件），返回 Arc 可跨任务克隆
    pub fn find(&self, key: &str) -> Option<Arc<dyn EnginePlugin>> {
        if let Some(engine) = self.builtin.get(key) {
            return Some(Arc::clone(engine));
        }
        for p in &self.plugins {
            if p.engine.plugin_id() == key || p.engine.display_name() == key {
                return Some(Arc::clone(&p.engine));
            }
        }
        None
    }

    /// 列出所有可用引擎的 (id, display_name)
    pub fn list_all(&self) -> Vec<(String, String)> {
        let mut all: Vec<_> = self
            .builtin
            .iter()
            .map(|(id, e)| (id.clone(), e.display_name().to_string()))
            .collect();
        for p in &self.plugins {
            all.push((p.engine.plugin_id().to_string(), p.engine.display_name().to_string()));
        }
        all.sort_by(|a, b| a.1.cmp(&b.1));
        all
    }

    /// 获取已加载的插件数量
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// 获取内置引擎数量
    pub fn builtin_count(&self) -> usize {
        self.builtin.len()
    }
}

impl Default for EngineRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── 内置引擎适配器 ──────────────────────────────────────────────────────
///
/// 将现有 `AggregationEngine` trait 实现适配为 `EnginePlugin`
/// 这样内置引擎和插件引擎可以通过统一接口调度。
pub struct BuiltinAdapter<T: crate::services::data_aggregator::AggregationEngine + Send + Sync> {
    inner: T,
    id: String,
}

impl<T: crate::services::data_aggregator::AggregationEngine + Send + Sync> BuiltinAdapter<T> {
    pub fn new(inner: T, id: &str) -> Self {
        Self {
            inner,
            id: id.to_string(),
        }
    }
}

impl<T: crate::services::data_aggregator::AggregationEngine + Send + Sync> EnginePlugin
    for BuiltinAdapter<T>
{
    fn plugin_id(&self) -> &str {
        &self.id
    }

    fn display_name(&self) -> &str {
        self.inner.name()
    }

    fn preview(&self, project: &Project) -> AppResult<PreviewData> {
        self.inner.preview(project)
    }

    fn execute(&self, project: &Project) -> AppResult<AggregationResult> {
        self.inner.execute(project)
    }
}

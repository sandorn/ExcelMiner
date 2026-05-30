//! 项目管理命令

use std::path::PathBuf;
use tauri::State;
use tokio::sync::Mutex;

use crate::config::AppConfig;
use crate::error::AppError;
use crate::models::project::{Project, ProjectConfig};

/// 全局应用状态
pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub current_project: Mutex<Option<Project>>,
    /// 最近一次汇总的结果（跨步骤共享）
    pub aggregation_results: Mutex<Vec<crate::models::analysis::AggregationResult>>,
    /// 最近一次 AI 分析的结果（跨步骤共享）
    pub analysis_results: Mutex<Vec<crate::models::analysis::AnalysisResult>>,
    /// 日志文件写入 guard（持有期间保证日志刷新到磁盘）
    pub _log_guard: Mutex<Option<tracing_appender::non_blocking::WorkerGuard>>,
}

/// 创建新项目
#[tauri::command]
pub async fn create_project(
    state: State<'_, AppState>,
    name: String,
    year: u32,
    month: u32,
    data_folder: String,
    output_file: String,
) -> Result<Project, AppError> {
    tracing::info!("创建项目: {} ({}年{}月)", name, year, month);

    // 从公司注册表自动填充公司列表
    let registry = crate::services::company_registry::company_registry();
    let mut companies = Vec::new();
    for c in &registry.insurance {
        companies.push(crate::models::project::Company {
            name: c.name.clone(),
            business_type: crate::models::project::BusinessType::Insurance,
            regions: vec![],
        });
    }
    for c in &registry.commercial {
        companies.push(crate::models::project::Company {
            name: c.name.clone(),
            business_type: crate::models::project::BusinessType::Commercial,
            regions: vec![],
        });
    }
    for h in &registry.hotel {
        companies.push(crate::models::project::Company {
            name: h.name.clone(),
            business_type: crate::models::project::BusinessType::Hotel,
            regions: vec![],
        });
    }

    let project = Project {
        name,
        year,
        month,
        data_folder: PathBuf::from(&data_folder),
        output_file: PathBuf::from(&output_file),
        companies,
        ytd_months: month,
        ai_config: Default::default(),
    };

    // 保存为 .toml 项目文件
    let project_path = project.output_file.with_extension("project.toml");
    let project_config = project.to_config();
    let content = toml::to_string_pretty(&project_config)
        .map_err(|e| AppError::Config(e.to_string()))?;
    std::fs::write(&project_path, content)?;

    // 更新最近项目（只锁一次）
    {
        let mut config = state.config.lock().await;
        let path_str = project_path.to_string_lossy().to_string();
        config.general.recent_projects.retain(|p| p != &path_str);
        config.general.recent_projects.insert(0, path_str);
        if config.general.recent_projects.len() > 10 {
            config.general.recent_projects.truncate(10);
        }
        config.save()?;
    }

    *state.current_project.lock().await = Some(project.clone());

    Ok(project)
}

/// 打开已有项目
#[tauri::command]
pub async fn open_project(
    state: State<'_, AppState>,
    path: String,
) -> Result<Project, AppError> {
    let path = PathBuf::from(&path);
    if !path.exists() {
        return Err(AppError::FileNotFound(path.to_string_lossy().to_string()));
    }

    let content = std::fs::read_to_string(&path)?;
    let config: ProjectConfig = toml::from_str(&content)?;
    let mut project = Project::from_config(config);

    // 如果项目文件中 companies 为空，从注册表自动填充
    if project.companies.is_empty() {
        tracing::info!("项目 companies 为空，从公司注册表自动填充");
        let registry = crate::services::company_registry::company_registry();
        let mut companies = Vec::new();
        for c in &registry.insurance {
            companies.push(crate::models::project::Company {
                name: c.name.clone(),
                business_type: crate::models::project::BusinessType::Insurance,
                regions: vec![],
            });
        }
        for c in &registry.commercial {
            companies.push(crate::models::project::Company {
                name: c.name.clone(),
                business_type: crate::models::project::BusinessType::Commercial,
                regions: vec![],
            });
        }
        for h in &registry.hotel {
            companies.push(crate::models::project::Company {
                name: h.name.clone(),
                business_type: crate::models::project::BusinessType::Hotel,
                regions: vec![],
            });
        }
        project.companies = companies;
        // 同步回写到 TOML
        let project_config = project.to_config();
        if let Ok(content) = toml::to_string_pretty(&project_config) {
            let _ = std::fs::write(&path, &content);
        }
    }

    *state.current_project.lock().await = Some(project.clone());

    tracing::info!("打开项目: {}", project.name);
    Ok(project)
}

/// 保存项目
#[tauri::command]
pub async fn save_project(
    state: State<'_, AppState>,
    project: Project,
) -> Result<(), AppError> {
    let project_path = project.output_file.with_extension("project.toml");
    let config = project.to_config();
    let content = toml::to_string_pretty(&config)
        .map_err(|e| AppError::Config(e.to_string()))?;
    std::fs::write(&project_path, content)?;

    *state.current_project.lock().await = Some(project);

    tracing::info!("项目已保存");
    Ok(())
}

/// 获取全局默认配置
#[tauri::command]
pub async fn get_default_config(
    state: State<'_, AppState>,
) -> Result<AppConfig, AppError> {
    let config = state.config.lock().await;
    Ok(config.clone())
}

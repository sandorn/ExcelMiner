//! 项目管理命令

use std::path::PathBuf;
use tauri::State;
use tokio::sync::Mutex;

use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::models::project::{Project, ProjectConfig};

/// 全局应用状态
pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub current_project: Mutex<Option<Project>>,
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

    let config = state.config.lock().await;
    let project = Project {
        name,
        year,
        month,
        data_folder: PathBuf::from(&data_folder),
        output_file: PathBuf::from(&output_file),
        companies: vec![],
        ytd_months: month,
        ai_config: Default::default(),
    };

    // 保存为 .toml 项目文件
    let project_path = project.output_file.with_extension("project.toml");
    let project_config = project.to_config();
    let content = toml::to_string_pretty(&project_config)
        .map_err(|e| AppError::Config(e.to_string()))?;
    std::fs::write(&project_path, content)?;

    // 更新最近项目
    let mut config = state.config.lock().await;
    let path_str = project_path.to_string_lossy().to_string();
    config.general.recent_projects.retain(|p| p != &path_str);
    config.general.recent_projects.insert(0, path_str);
    if config.general.recent_projects.len() > 10 {
        config.general.recent_projects.truncate(10);
    }
    config.save()?;

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
    let project = Project::from_config(config);

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

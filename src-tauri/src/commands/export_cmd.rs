//! 报表导出命令

use std::process::Command;

use tauri::State;
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::commands::project_cmd::AppState;
use crate::error::AppError;
use crate::services::report_writer::ReportWriter;

/// 导出报表（将汇总数据 + AI分析写入 xlsx）
#[tauri::command]
pub async fn export_report(
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let project_guard = state.current_project.lock().await;
    let project = project_guard
        .as_ref()
        .ok_or_else(|| AppError::Other("请先打开或创建项目".into()))?;

    let output_path = project.output_file.to_string_lossy().to_string();
    let output_path = output_path.replace('\\', "/");
    tracing::info!("导出报表到: {}", output_path);

    // 从 AppState 读取实际汇总和分析结果
    let agg_results = state.aggregation_results.lock().await;
    let ai_results = state.analysis_results.lock().await;

    ReportWriter::write_summary(
        &project.output_file,
        &agg_results,
        &ai_results,
        &project.name,
        project.year,
        project.month,
    )?;

    Ok(output_path)
}

/// 复制文本到剪贴板（使用 Tauri clipboard 插件，安全无注入风险）
#[tauri::command]
pub async fn copy_to_clipboard(
    app_handle: tauri::AppHandle,
    text: String,
) -> Result<(), AppError> {
    app_handle
        .clipboard()
        .write_text(text)
        .map_err(|e| AppError::Other(format!("剪贴板操作失败: {}", e)))
}

/// 在文件浏览器中打开文件夹
#[tauri::command]
pub async fn open_in_explorer(path: String) -> Result<(), AppError> {
    let result = if cfg!(target_os = "windows") {
        Command::new("explorer").arg(&path).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(&path).spawn()
    } else {
        Command::new("xdg-open").arg(&path).spawn()
    };

    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(AppError::Other(format!("打开文件夹失败: {}", e))),
    }
}

/// 打开日志文件所在文件夹（用文件浏览器定位）
#[tauri::command]
pub async fn open_log_folder() -> Result<String, AppError> {
    let log_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("ExcelMiner")
        .join("logs");

    let log_path = log_dir.to_string_lossy().to_string();

    // 在文件浏览器中打开日志目录
    if cfg!(target_os = "windows") {
        Command::new("explorer").arg(&log_path).spawn()
            .map_err(|e| AppError::Other(format!("打开日志文件夹失败: {}", e)))?;
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(&log_path).spawn()
            .map_err(|e| AppError::Other(format!("打开日志文件夹失败: {}", e)))?;
    } else {
        Command::new("xdg-open").arg(&log_path).spawn()
            .map_err(|e| AppError::Other(format!("打开日志文件夹失败: {}", e)))?;
    }

    Ok(log_path)
}

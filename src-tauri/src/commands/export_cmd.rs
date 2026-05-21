//! 报表导出命令

use std::process::Command;

use tauri::State;

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
    tracing::info!("导出报表到: {}", output_path);

    // TODO: 从 state 中获取实际汇总数据
    ReportWriter::write_summary(
        &project.output_file,
        &[],     // aggregation_results
        &[],     // ai_results
        &project.name,
        project.year,
        project.month,
    )?;

    Ok(output_path)
}

/// 复制文本到剪贴板（通过 PowerShell）
#[tauri::command]
pub async fn copy_to_clipboard(text: String) -> Result<(), AppError> {
    let output = if cfg!(target_os = "windows") {
        Command::new("powershell")
            .args(["-Command", &format!("Set-Clipboard -Value '{}'", text.replace('\'', "''"))])
            .output()
    } else if cfg!(target_os = "macos") {
        Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child.stdin.as_mut().unwrap().write_all(text.as_bytes())?;
                child.wait_with_output()
            })
    } else {
        Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child.stdin.as_mut().unwrap().write_all(text.as_bytes())?;
                child.wait_with_output()
            })
    };

    match output {
        Ok(_) => {
            tracing::info!("已复制到剪贴板");
            Ok(())
        }
        Err(e) => Err(AppError::Other(format!("剪贴板操作失败: {}", e))),
    }
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

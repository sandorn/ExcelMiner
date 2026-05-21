pub mod commands;
pub mod config;
pub mod error;
pub mod models;
pub mod services;
pub mod utils;

use tauri::Manager;
use tracing_subscriber;

/// 应用入口
pub fn run() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "excelminer=info".into()),
        )
        .init();

    tracing::info!("ExcelMiner 启动中...");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            tracing::info!("ExcelMiner 初始化完成");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 项目命令
            commands::project_cmd::create_project,
            commands::project_cmd::open_project,
            commands::project_cmd::save_project,
            commands::project_cmd::get_default_config,
            // 导入命令
            commands::import_cmd::preview_import,
            commands::import_cmd::execute_aggregation,
            // 分析命令
            commands::analysis_cmd::execute_analysis,
            commands::analysis_cmd::test_api_connection,
            // 导出命令
            commands::export_cmd::export_report,
            commands::export_cmd::copy_to_clipboard,
            commands::export_cmd::open_in_explorer,
        ]);

    app.build(tauri::generate_context!())
        .expect("error building tauri application")
        .run(|_app_handle, _event| {});
}

pub mod commands;
pub mod config;
pub mod error;
pub mod models;
pub mod services;
pub mod utils;

use tauri::Manager;
use tracing_subscriber;

use commands::project_cmd::AppState;
use config::AppConfig;
use tokio::sync::Mutex;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// 应用入口
pub fn run() {
    // 日志文件路径: %APPDATA%/ExcelMiner/logs/app.log
    let log_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("ExcelMiner")
        .join("logs");
    std::fs::create_dir_all(&log_dir).ok();

    let file_appender = tracing_appender::rolling::never(&log_dir, "app.log");
    let (non_blocking, log_guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "excelminer=info".into());

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())          // 终端输出
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking)) // 文件输出
        .init();

    tracing::info!("ExcelMiner 启动中... 日志文件: {}", log_dir.join("app.log").display());

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let config = AppConfig::load().unwrap_or_default();
            let state = AppState {
                config: Mutex::new(config),
                current_project: Mutex::new(None),
                aggregation_results: Mutex::new(Vec::new()),
                analysis_results: Mutex::new(Vec::new()),
                _log_guard: Mutex::new(Some(log_guard)),
            };
            app.manage(state);
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
            commands::export_cmd::open_log_folder,
        ]);

    app.build(tauri::generate_context!())
        .expect("error building tauri application")
        .run(|_app_handle, _event| {});
}

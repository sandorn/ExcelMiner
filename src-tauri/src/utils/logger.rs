//! 日志初始化与轮转工具
//!
//! 项目在启动时会创建每日日志文件，并在日志总大小超过阈值时自动清理最旧的 20% 文件。
//! 该模块提供 `init_logger`，在 `lib.rs::run` 中调用即可完成初始化。

use std::path::Path;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// 清理超量日志文件：当日志目录总大小超过 `max_total` 时，删除最旧的 20% 文件。
pub fn cleanup_old_logs(log_dir: &Path, max_total: u64) {
    let Ok(entries) = std::fs::read_dir(log_dir) else { return };

    let mut files: Vec<(std::path::PathBuf, u64)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.starts_with("ExcelMiner.") && n.ends_with(".log"))
                .unwrap_or(false)
        })
        .map(|e| {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            (e.path(), size)
        })
        .collect();

    files.sort_by_key(|(p, _)| p.to_string_lossy().to_string());
    let total: u64 = files.iter().map(|(_, s)| s).sum();
    if total > max_total {
        let to_delete = ((files.len() as f64) * 0.2).ceil() as usize;
        for (path, _) in files.iter().take(to_delete) {
            std::fs::remove_file(path).ok();
        }
    }
}

/// 初始化全局日志记录器。
/// - 日志文件位于 `%APPDATA%/ExcelMiner/logs/ExcelMiner.YYYYMMDD.log`
/// - 同时输出到终端，使用 `tracing_subscriber` 的 fmt layer。
pub fn init_logger() {
    let log_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("ExcelMiner")
        .join("logs");
    std::fs::create_dir_all(&log_dir).ok();

    // 清理旧日志，阈值 100 MB
    cleanup_old_logs(&log_dir, 100 * 1024 * 1024);

    let date_str = chrono::Local::now().format("%Y%m%d").to_string();
    let log_filename = format!("ExcelMiner.{}.log", date_str);
    let file_appender = tracing_appender::rolling::never(&log_dir, &log_filename);
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "excelminer=info".into());

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer()) // terminal output
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking)) // file output
        .init();

    tracing::info!("日志初始化完成，文件: {}", log_dir.join(&log_filename).display());
}

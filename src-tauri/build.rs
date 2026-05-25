use std::path::Path;

fn main() {
    // 从项目根 resources/ 复制提示词和配置到 src-tauri/resources/（供 Tauri 打包）
    let src = Path::new("../resources");
    let dst = Path::new("resources");
    if src.exists() {
        // 确保目标目录存在
        let _ = std::fs::create_dir_all(dst.join("prompts"));
        // 复制 companies.toml
        let _ = std::fs::copy(src.join("companies.toml"), dst.join("companies.toml"));
        // 复制 prompts/*.md
        if let Ok(entries) = std::fs::read_dir(src.join("prompts")) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let _ = std::fs::copy(entry.path(), dst.join("prompts").join(&name));
            }
        }
    }
    tauri_build::build()
}

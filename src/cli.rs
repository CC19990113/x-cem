//! CLI 的 IO 层：目录遍历、读文件、路径展示。
//!
//! 解析全部走 `x_cem` 的纯函数（[`x_cem::analyze_many`]），本模块只负责把
//! 文件系统上的 `.ts` / `.js` 读成 `(path, content)` 再交给核心，核心本身
//! 不依赖 `std::fs` / `std::env`。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use x_cem::analyze_many;
use x_cem::emit::Manifest;

/// 递归扫描目录（也接受单个文件），额外返回跨文件关联的 warning。
///
/// 文件按路径排序，输出稳定；隐藏目录与 `node_modules` / `target` /
/// `dist` / `coverage` 会被跳过，符号链接不跟随。
pub fn scan_dir_with_warnings(src: &Path) -> io::Result<(Manifest, Vec<String>)> {
    let root = src
        .canonicalize()
        .map_err(|err| io::Error::new(err.kind(), format!("无法访问 {}: {err}", src.display())))?;
    let cwd = std::env::current_dir().ok();

    let mut inputs: Vec<(String, String)> = Vec::new();
    if root.is_file() {
        let parent = root.parent().unwrap_or_else(|| Path::new("."));
        inputs.push(read_source(&root, parent, cwd.as_deref())?);
    } else {
        for file in collect_files(&root)? {
            inputs.push(read_source(&file, &root, cwd.as_deref())?);
        }
    }

    let borrowed: Vec<(&str, &str)> = inputs
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let (manifest, warnings) = analyze_many(&borrowed);
    Ok((manifest, warnings))
}

/// 读一个文件，返回（展示路径, 源码）。错误文案与旧版逐字一致。
fn read_source(file: &Path, root: &Path, cwd: Option<&Path>) -> io::Result<(String, String)> {
    let text = fs::read_to_string(file).map_err(|err| {
        io::Error::new(err.kind(), format!("读取 {} 失败: {err}", file.display()))
    })?;
    Ok((display_path(file, root, cwd), text))
}

/// 优先输出相对 cwd 的路径；src 在 cwd 之外时退回相对 src 的路径。
fn display_path(file: &Path, root: &Path, cwd: Option<&Path>) -> String {
    let relative = cwd
        .and_then(|cwd| file.strip_prefix(cwd).ok())
        .or_else(|| file.strip_prefix(root).ok())
        .unwrap_or(file);
    relative.to_string_lossy().replace('\\', "/")
}

fn collect_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)?.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.')
                || matches!(
                    name.as_ref(),
                    "node_modules" | "target" | "dist" | "coverage"
                )
            {
                continue;
            }
            walk(&entry.path(), files)?;
        } else if file_type.is_file() {
            let is_source = entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "ts" || ext == "js");
            if is_source {
                files.push(entry.path());
            }
        }
        // 符号链接直接跳过，避免目录环。
    }
    Ok(())
}

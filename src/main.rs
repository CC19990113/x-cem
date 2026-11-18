//! x-cem CLI。参数解析手写（不引 clap）。
//!
//! 所有 IO（参数解析、目录遍历、读文件、写输出）都留在这里与 [`cli`] 模块，
//! 解析核心 `x_cem` 是纯函数，可被原生 CLI 与 wasm 两侧复用。

mod cli;

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use cli::scan_dir_with_warnings;

const USAGE: &str = "\
x-cem — 词法级 TS/JS Custom Elements Manifest 生成器

用法:
  x-cem scan --src <目录或文件> --out <输出文件>

选项:
  --src <path>    递归扫描 *.ts / *.js
  --out <path>    输出 CEM JSON；`-` 表示 stdout
  -h, --help      显示帮助
  -V, --version   显示版本
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("x-cem: {err}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        return Err(usage_error("缺少子命令"));
    };
    match command.as_str() {
        "-h" | "--help" => {
            println!("{USAGE}");
            Ok(())
        }
        "-V" | "--version" => {
            println!("x-cem {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "scan" => scan(&args[1..]),
        other => Err(usage_error(&format!("未知子命令 `{other}`"))),
    }
}

fn scan(args: &[String]) -> Result<(), String> {
    let mut src: Option<String> = None;
    let mut out: Option<String> = None;
    let mut i = 0usize;
    while i < args.len() {
        let arg = &args[i];
        let (key, inline_value) = match arg.split_once('=') {
            Some((key, value)) => (key, Some(value.to_string())),
            None => (arg.as_str(), None),
        };
        match key {
            "--src" | "--out" => {
                let value = match inline_value {
                    Some(value) => value,
                    None => {
                        i += 1;
                        args.get(i)
                            .cloned()
                            .ok_or_else(|| usage_error(&format!("`{key}` 缺少取值")))?
                    }
                };
                if key == "--src" {
                    src = Some(value);
                } else {
                    out = Some(value);
                }
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(usage_error(&format!("未知参数 `{other}`"))),
        }
        i += 1;
    }

    let src = src.ok_or_else(|| usage_error("缺少 `--src`"))?;
    let out = out.ok_or_else(|| usage_error("缺少 `--out`"))?;
    let (manifest, warnings) = scan_dir_with_warnings(Path::new(&src))
        .map_err(|err| format!("扫描 `{src}` 失败: {err}"))?;
    for warning in &warnings {
        eprintln!("x-cem: 警告: {warning}");
    }
    let json = manifest.to_json();

    if out == "-" {
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(json.as_bytes())
            .map_err(|err| format!("写入 stdout 失败: {err}"))?;
    } else {
        std::fs::write(&out, json.as_bytes()).map_err(|err| format!("写入 `{out}` 失败: {err}"))?;
    }

    let declaration_count: usize = manifest
        .modules
        .iter()
        .map(|module| module.declarations.len())
        .sum();
    eprintln!(
        "x-cem: {} 个模块 / {} 个声明 -> {}",
        manifest.modules.len(),
        declaration_count,
        out
    );
    Ok(())
}

fn usage_error(message: &str) -> String {
    format!("{message}\n\n{USAGE}")
}

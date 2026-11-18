//! 1000 行 TS 文件的计时基准。只依赖 std，用 `Instant` 自己计时。
//!
//! 运行：
//!
//! ```sh
//! cargo run --release --bin xcem-bench
//! ```
//!
//! 生成 `/tmp/xcem-bench/bench-1000.ts`，分别测：
//! - 仅文件读取
//! - 仅词法分析 `lex`
//! - 端到端（读文件 + lex + 识别 + JSON 序列化）
//! - 纯管线（端到端减去读取）

use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use x_cem::{lexer, manifest_for_source};

const RUNS: usize = 200;
const TARGET_LINES: usize = 1000;

fn main() {
    let path = prepare_fixture();
    let source = fs::read_to_string(&path).expect("读取基准文件失败");
    println!("基准文件: {}", path.display());
    println!(
        "行数: {}  字节: {}  （每个样本 warmup 后重复 {} 次）",
        source.lines().count(),
        source.len(),
        RUNS
    );

    for _ in 0..50 {
        black_box(lexer::lex(&source));
        let module = manifest_for_source("bench.ts", &source).expect("基准文件应有声明");
        black_box(module.to_json());
    }

    let mut read_samples = Vec::with_capacity(RUNS);
    let mut lex_samples = Vec::with_capacity(RUNS);
    let mut e2e_samples = Vec::with_capacity(RUNS);

    for _ in 0..RUNS {
        let start = Instant::now();
        let text = fs::read_to_string(&path).expect("读取基准文件失败");
        read_samples.push(start.elapsed());
        black_box(text);

        let start = Instant::now();
        black_box(lexer::lex(&source));
        lex_samples.push(start.elapsed());

        let start = Instant::now();
        let text = fs::read_to_string(&path).expect("读取基准文件失败");
        let module = manifest_for_source("bench.ts", &text).expect("基准文件应有声明");
        let json = module.to_json();
        black_box(json.len());
        e2e_samples.push(start.elapsed());
    }

    let pipeline_samples: Vec<Duration> = e2e_samples
        .iter()
        .zip(&read_samples)
        .map(|(all, read)| all.saturating_sub(*read))
        .collect();

    report("仅文件读取", &read_samples);
    report("仅词法 lex", &lex_samples);
    report("纯管线(lex+识别+JSON)", &pipeline_samples);
    report("端到端(读文件+全部)", &e2e_samples);
}

fn report(label: &str, samples: &[Duration]) {
    let mut nanos: Vec<u128> = samples.iter().map(Duration::as_nanos).collect();
    nanos.sort_unstable();
    let n = nanos.len();
    let min = nanos[0] as f64 / 1e6;
    let median = nanos[n / 2] as f64 / 1e6;
    let mean = nanos.iter().sum::<u128>() as f64 / n as f64 / 1e6;
    let p95 = nanos[(n * 95 / 100).min(n - 1)] as f64 / 1e6;
    println!("{label:<24} min {min:7.4} ms | median {median:7.4} ms | mean {mean:7.4} ms | p95 {p95:7.4} ms");
}

fn prepare_fixture() -> PathBuf {
    let dir = std::env::temp_dir().join("xcem-bench");
    fs::create_dir_all(&dir).expect("创建临时目录失败");
    let path = dir.join("bench-1000.ts");
    let source = build_source(TARGET_LINES);
    assert_eq!(
        source.lines().count(),
        TARGET_LINES,
        "基准文件必须是 1000 行"
    );
    fs::write(&path, source).expect("写入基准文件失败");
    path
}

/// 每块 27 行：JSDoc + class + define，补注释凑满 1000 行。
fn build_source(target: usize) -> String {
    let mut out = String::with_capacity(target * 48);
    let mut lines = 0usize;
    let mut index = 0usize;
    while lines < target {
        let block = block(index);
        let block_lines = block.lines().count();
        if lines + block_lines > target {
            break;
        }
        lines += block_lines;
        out.push_str(&block);
        index += 1;
    }
    while lines < target {
        out.push_str("// -----------------------------------------------------------------\n");
        lines += 1;
    }
    out
}

fn block(index: usize) -> String {
    format!(
        "/**\n * `<x-bench-{index}>` 基准组件。\n *\n * @attr {{('a'|'b')}} [variant='a'] - 变体\n * @attr {{boolean}} [disabled=false] - 禁用\n * @slot - 内容\n * @cssprop --x-bench-height - 高度\n * @csspart base - 内部节点\n * @fires x-bench-{index}-click - 点击\n */\nexport class XBench{index} extends HTMLElement {{\n  static observedAttributes = ['variant', 'disabled'];\n\n  constructor() {{\n    super();\n    this.attachShadow({{ mode: 'open' }}).innerHTML = `\n      <style>\n        :host {{ display: inline-block; }}\n        .base {{ height: var(--x-bench-height, 2rem); }}\n      </style>\n      <button part=\"base\"><slot></slot></button>\n    `;\n  }}\n}}\n\ncustomElements.define('x-bench-{index}', XBench{index});\n\n",
        index = index
    )
}

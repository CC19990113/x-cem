# x-cem 性能基准

目标：1000 行 TS 单文件的扫描耗时 **< 5ms**。

## 环境

| 项 | 值 |
| --- | --- |
| 机器 | Apple M2 / macOS（arm64） |
| 默认工具链 | rustc 1.93.0，host `x86_64-apple-darwin`（x86_64 二进制经 Rosetta 2 运行） |
| 原生对照工具链 | `stable-aarch64-apple-darwin`（cargo 1.96.0），仅用于计时对照，仓库默认构建命令不变 |
| 构建 | release：`opt-level = 3` + thin LTO |
| 第三方依赖 | 0（只有 std） |

## 方法

`src/bin/xcem-bench.rs` 会生成一份 **1000 行 / 25,142 字节** 的 TS 文件（37 个带完整
JSDoc 的组件块，路径为 `std::env::temp_dir()/xcem-bench/bench-1000.ts`），warmup 50 次后
用 `std::time::Instant` 采样 200 次，分别统计文件读取、词法分析、纯管线与端到端。

```sh
cargo build --release
./target/release/xcem-bench
```

CLI 墙钟用 Python `subprocess` 连续启动 100 次统计（包含进程启动、扫描、写 JSON）。
采样时机器有并发负载（load average ≈ 10），mean / p95 会被调度尖刺拉高，看 median 更稳。

## 进程内测量（默认 x86_64 构建，200 次采样）

| 阶段 | min | median | mean | p95 |
| --- | ---: | ---: | ---: | ---: |
| 仅文件读取 | 0.0152 ms | 0.0220 ms | 0.0258 ms | 0.0437 ms |
| 仅词法 lex | 0.0364 ms | 0.0493 ms | 0.0520 ms | 0.0719 ms |
| 纯管线（lex + 识别 + JSON） | 0.1512 ms | **0.2289 ms** | 0.2585 ms | 0.3891 ms |
| 端到端（读文件 + 全部，进程内） | 0.1910 ms | **0.2517 ms** | 0.2842 ms | 0.5258 ms |

1000 行文件解析出 37 个 `custom-element-definition` 对应的 class 声明。
多次重复运行纯管线的 median 在 0.22~0.23ms 之间波动。

## CLI 墙钟（100 次，含进程启动）

| 构建 / 场景 | min | median | mean | p95 |
| --- | ---: | ---: | ---: | ---: |
| 默认 x86_64（Rosetta）/ scan 1000 行 | 10.38 ms | 12.40 ms | 12.74 ms | 15.74 ms |
| 默认 x86_64（Rosetta）/ `--version`（零扫描） | 9.96 ms | 13.03 ms | 13.40 ms | 16.04 ms |
| 原生 arm64 / scan 1000 行 | 2.66 ms | **3.09 ms** | 4.30 ms | 5.77 ms |
| 原生 arm64 / scan fixture（63 行） | 2.42 ms | 2.71 ms | 3.06 ms | 4.71 ms |
| 原生 arm64 / `--version`（零扫描） | 2.44 ms | 3.03 ms | 3.33 ms | 4.84 ms |

## 关于 Rosetta 启动底噪（重要）

默认工具链编译出的是 **x86_64** 二进制，在 arm64 机器上经 Rosetta 2 启动，单次进程启动
固定要 10~14ms。这跟 x-cem 做了什么无关：

| 对照程序 | 架构 | 启动 median |
| --- | --- | ---: |
| C hello world（clang） | arm64 原生 | 2.67 ms |
| C hello world（clang） | x86_64（Rosetta） | 14.39 ms |
| Rust hello world（rustc） | x86_64（Rosetta） | 13.74 ms |
| `x-cem --version` | x86_64（Rosetta） | 13.03 ms |
| `x-cem --version` | arm64 原生 | 3.03 ms |

因此结论分两层：

- **工具自身耗时**：1000 行文件词法 0.049ms、纯管线 **0.229ms**，远低于 5ms。
- **CLI 墙钟**：原生 arm64 构建 median **3.09ms**、min 2.66ms，满足 < 5ms；默认
  x86_64 构建的 12.4ms 是 Rosetta 启动成本，任何同架构二进制都不可能低于 5ms
  （hello world 也要 13~14ms）。

复现原生对照（不改变仓库默认构建）：

```sh
cargo +stable-aarch64-apple-darwin build --release
cp target/release/x-cem /tmp/x-cem-arm64
/tmp/x-cem-arm64 scan --src <dir> --out /tmp/cem.json
cargo build --release   # 恢复默认工具链的 x86_64 产物
```

## 与 @custom-elements-manifest/analyzer 0.11.0 的结构对拍

analyzer 临时装在 `/tmp/xcem-analyzer`（未进 `Cargo.toml`），对同一份 fixture 执行：

```sh
cd /tmp/xcem-compare
/tmp/xcem-analyzer/node_modules/.bin/cem analyze --globs "x-button.ts"
```

对比 `custom-elements.json` 与 `x-cem scan` 的输出：

- `name` / `tagName` / `customElement` 等价。
- `events` / `slots` / `cssProperties` / `cssParts` 的名称与描述逐条等价。
- `attributes` 的 name / type.text / description 逐条等价；`default` 仅引号风格不同：
  analyzer 保留字面量引号（`'default'`），本工具按任务样例去引号（`default`）。
- analyzer 额外输出 `description` / `members` / `superclass`，且 `schemaVersion` 为
  `1.0.0`；本工具按任务约定只输出 CEM 2.1.0 的声明子集。

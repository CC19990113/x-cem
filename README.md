# x-cem

**零依赖、词法级（不建 AST）的 Custom Elements Manifest 生成器。**

从 TS/JS 源码提取 [Custom Elements Manifest 2.1.0](https://github.com/webcomponents/custom-elements-manifest)
子集：`class`、`tagName`、`attributes`、`properties`、`events`、`slots`、`cssProperties`、`cssParts`。

不依赖 TypeScript 编译器，不建 AST，不做类型解析，不做模块解析。
`[dependencies]` 为空，原生 CLI 与 wasm 产物共用同一套 Rust 代码。

## 性能

同一份输入（1000 行 TS / 37 个带完整 JSDoc 的组件）的**纯解析管线**耗时：

| 实现 | 解析方式 | 耗时 |
| --- | --- | --- |
| **x-cem 0.1.0**（原生 release） | 手写词法扫描，只识别 `define` / `class` / JSDoc，不建 AST | **0.229 ms**（本机实测） |
| `@custom-elements-manifest/analyzer` 0.11.0 | TypeScript 编译器 API，解析完整 TS AST | 未在本仓库实测；**量级差异**，依据见下 |

**口径与依据**（避免误读，先把话说清楚）：

- `0.229 ms` 来自 [`benches/README.md`](benches/README.md)：Apple M2 / release（`opt-level = 3` + thin LTO），
  warmup 50 次后采样 200 次；只统计**解析管线**，不含进程启动、文件读取和 JSON 落盘。
- analyzer 的耗时**不是本仓库实测**。依据是 analyzer 0.11.0 实际依赖
  `typescript@~5.4.2`，源码直接 `import ts from 'typescript'` 并解析完整 AST
  （按 npm 包内容核实；顺带纠正一个常见说法：0.11.0 **并不使用** ts-morph）。
  本工具只做一次线性词法扫描，因此两者是量级差异；没有把 analyzer 装进同一 benchmark，
  所以这里不写具体耗时数字。需要精确同机对比时请自行运行 analyzer 的基准。
- 输出等价性用 fixture 做过对照：`attributes` 的 name/type/description、`events`、`slots`、
  `cssProperties`、`cssParts` 逐条等价；`default` 取值一致（x-cem 输出去引号的裸值）。
  analyzer 额外输出 `members` / `superclass`，且 `schemaVersion` 为 `1.0.0`；x-cem 按 CEM 2.1.0
  输出声明子集（`0.1.2` 起含 `declarations[].description` 与结构化 `default`）。

## 安装与用法

### npx（无需 Rust / 无需编译）

```sh
npx x-cem scan --src src --out custom-elements-manifest.json
```

或作为开发依赖安装：

```sh
npm i -D x-cem
npx x-cem scan --src src --out custom-elements-manifest.json
```

npm 包里自带一份预编译的 `core.wasm`，Windows / macOS / Linux 通用，要求 Node.js >= 18。

### cargo install（原生 CLI）

```sh
cargo install x-cem
x-cem scan --src src --out custom-elements-manifest.json
```

### CLI 参数

```text
x-cem scan --src <目录或文件> [--out <输出文件>]

--src <path>    递归扫描 *.ts / *.js（跳过 node_modules、target、dist、coverage 和隐藏目录）
--out <path>    输出 CEM JSON；缺省或 `-` 表示 stdout
```

退出码（npm 版）：`0` 成功，`1` 扫描/解析失败，`2` 用法错误；原生 CLI 的错误统一以 `2` 退出。
关联阶段的 warning 打到 stderr。

### Node API

```js
import { readFile } from 'node:fs/promises';
import { scanSource, scanDir } from 'x-cem';

const source = await readFile('src/x-button.ts', 'utf8');
const module = await scanSource({ path: 'src/x-button.ts', source });

const manifest = await scanDir('src');
// { schemaVersion: '2.1.0', modules: [...] }
```

`scanDir` 逐文件调用 wasm，再拼成 CEM 根对象；`scanSource` 只处理单个源文件。
需要拿到关联 warning 时用 `scanDirWithWarnings(dir)`，返回 `{ manifest, warnings }`。

## 输出示例

下面是 x-cem 扫真实 `x-button` 组件的输出（为便于阅读，省略了部分 attributes 与 cssProperties）：

```json
{
  "schemaVersion": "2.1.0",
  "modules": [
    {
      "kind": "javascript-module",
      "path": "button/x-button.ts",
      "declarations": [
        {
          "kind": "class",
          "name": "XButton",
          "description": "`<x-button>` — an action element backed by a real native `<button>` inside the shadow root, ...",
          "tagName": "x-button",
          "customElement": true,
          "attributes": [
            {
              "name": "variant",
              "type": { "text": "('default'|'primary'|'danger'|'ghost')" },
              "description": "Visual intent. Default: \"default\".",
              "default": "default"
            },
            {
              "name": "size",
              "type": { "text": "('sm'|'md'|'lg')" },
              "description": "Control size. Default: \"md\".",
              "default": "md"
            }
          ],
          "events": [
            {
              "name": "x-button-click",
              "description": "Fired when the user activates the button; `detail.originalEvent` is the native MouseEvent."
            }
          ],
          "slots": [
            {
              "name": "",
              "description": "Default slot: the button label (text, icon, or both)."
            }
          ],
          "cssProperties": [
            {
              "name": "--x-button-height",
              "description": "Control height; defaults to 2.25rem (2rem sm, 2.75rem lg)."
            }
          ],
          "cssParts": [
            {
              "name": "base",
              "description": "The internal native `<button>` element."
            }
          ]
        }
      ]
    }
  ]
}
```

## 它不做什么（诚实的限制清单）

这些是设计取舍，不是待办列表。遇到它们时 x-cem 会**静默跳过或报 warning**，
不会假装自己做了完整类型分析：

1. **import 别名不关联**：`import { A as B }` 后 `define('x', B)` 会失配。
2. **装饰器注册不识别**：`@customElement('x')` 不会产生 `tagName`；而且装饰器会打断 JSDoc 的紧邻判定。
3. **匿名类不识别**：`define('x', class extends HTMLElement {})` 不会被收集。
4. **动态 tag 不识别**：变量、`ce.define` 别名、`customElements['define']` 都不认。
5. **同名 class 跨模块 + 跨文件 define 不消歧**：报歧义，不分配 `tagName`（不做模块图分析）。
6. **正则 / 除号启发式**：在极少数对象字面量后仍可能误判词法边界。
7. **扫描范围有限**：只扫 `.ts` / `.js`，不支持 `.tsx` / `.mts` / `.cjs`；
   输出是 CEM 子集，**没有** `members`、`superclass`、`exports`。

## 项目结构

```text
src/
  lexer.rs          手写 TS/JS 词法器
  jsdoc.rs          JSDoc 子集解析（@attr / @slot / @cssprop / @csspart / @fires）
  lib.rs            单文件分析 + 跨文件 define 关联
  emit.rs           CEM 2.1.0 JSON 手写序列化
  main.rs / cli.rs  原生 CLI
  wasm_api.rs       wasm C ABI 导出
  bin/xcem-bench.rs 性能基准
npm/                npm 包壳：Node 读文件/遍历目录，wasm 里解析
tests/              集成测试与 fixture
benches/            基准方法与数据
```

## 参与

```sh
cargo test                 # 运行测试
cargo run -- scan --src tests/fixtures/x-button.ts --out -
cargo build --release      # 原生 CLI
```

构建 wasm（npm 包用）：

```sh
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/x_cem.wasm npm/core.wasm
```

若仓库根目录提供 `pnpm build:wasm` 脚本，直接运行它即可。npm 壳零依赖，核心逻辑只有
`cli.js` / `index.js` / `wasm-loader.js` 三个 JS 文件；发布流程见 [`PUBLISHING.md`](PUBLISHING.md)。

## License

MIT

# x-cem

零依赖、词法级（不建 AST）的 Custom Elements Manifest 2.1.0 生成器。
npm 包内置一份跨平台 `core.wasm`，Node.js >= 18，无需 Rust 工具链。

## 用法

```sh
npx x-cem scan --src src --out custom-elements-manifest.json
# 不写 --out 时输出到 stdout
npx x-cem scan --src src
```

Node API：

```js
import { scanSource, scanDir } from 'x-cem';

const manifest = await scanDir('src');
// { schemaVersion: '2.1.0', modules: [...] }
```

`scanSource({ path, source })` 解析单个源文件。完整文档、性能对比和限制清单见
[GitHub 仓库](https://github.com/CC19990113/x-cem)。

## 限制

import 别名、装饰器注册、匿名类、动态 tag、跨模块同名 class 消歧、`.tsx` / `.mts` / `.cjs`
均不支持；输出是 CEM 子集（无 `members` / `superclass` / `exports`）。详见仓库 README 的
「它不做什么」。

## License

MIT

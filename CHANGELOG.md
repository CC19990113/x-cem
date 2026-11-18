# Changelog

## 0.1.2 — CEM 补 declaration.description 与结构化 default

下游 AI 层实测发现两个字段缺口，本次补齐。

### 新增

- **`declarations[].description`**：取 class 紧邻 JSDoc 的首段（第一个空行之前、任何标签之前的自然段），多行用空格拼接。无 JSDoc、首段为空、或 JSDoc 直接从标签开始时**不输出该字段**（不会输出空字符串）。
- **`attributes[].default`（结构化）**：除原有的 `[name=value]` 写法外，新增两种来源：
  1. 显式 JSDoc 赋值：`@attr {('sm'|'md'|'lg')} size = md - 说明`；
  2. 描述文本兜底：`Default: "md"` / `Default: md`（大小写不敏感，引号可选）。

  解析优先级：显式 JSDoc 默认值（`[name=value]` / `name = value`）> 描述文本里的 `Default:`。取到的值**去掉引号**，与 0.1.1 行为一致；两条都取不到就不输出 `default`，布尔属性不会被补成 `false`。

### 兼容性

- wasm ABI 不变：5 个导出、0 个 import，Node 侧无需改动。
- 纯新增字段，不改任何已有字段的语义；已存在的 `[name=value]` 写法行为不变。

## 0.1.1 — 修复 npm 首发 tarball 缺失

- 重新发布，修复 `x-cem@0.1.0` 在 npm 上只有 registry 元数据、tarball 404（`https://registry.npmjs.org/x-cem/-/x-cem-0.1.0.tgz`）的问题。
- 无功能变更：`npm/core.wasm` 与 0.1.0 保持一致，仅版本号提升。

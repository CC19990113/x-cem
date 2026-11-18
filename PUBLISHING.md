# 发布指南

> **硬规则：agent 不得自动执行任何发布动作。**
> `npm publish`、`cargo publish`、`git tag`、`git push` 必须由人类在确认后手动执行。
> agent 只可以准备文件、跑 `--dry-run`、列出检查项。

## 0. 发布前检查清单

- [ ] `cargo test` 全绿
- [ ] 工作区干净：`git status --short` 无未提交改动
- [ ] 版本号同步（见第 3 节）：`Cargo.toml`、`npm/package.json`、`CHANGELOG.md`
- [ ] `npm/core.wasm` 是**当前版本**构建的产物（不是旧缓存）：
      `cargo build --release --target wasm32-unknown-unknown` 后拷贝
- [ ] `cd npm && npm pack --dry-run` 的 Tarball Contents 里能看到 `core.wasm`
- [ ] `npm/package.json` 的 `"dependencies"` 仍为空对象（零依赖红线）
- [ ] 仓库有 MIT `LICENSE` 文件（当前缺失，首次发布前应补上）
- [ ] 首次发布前确认名称归属：`npm view x-cem` / `cargo search x-cem` 返回 404（或确认属于本项目）
- [ ] 人工确认：**同意发布版本 `<version>`**

## 1. npm 发布

发布目录是 `npm/`，不是仓库根。`files` 白名单已经限定内容，`package.json` 会由 npm 自动带上。

```sh
cd npm

# 身份与内容确认
npm whoami
npm pack --dry-run

# 真正发布（需要人类输入 2FA / OTP，或使用已登录的 token）
npm publish --registry=https://registry.npmjs.org
```

发布后验证：

```sh
npm view x-cem@<version> version dist.tarball
cd /tmp && npx -y x-cem@<version> scan --src <某项目>/src --out /tmp/cem-check.json
python3 -m json.tool /tmp/cem-check.json > /dev/null && echo "JSON ok"
```

注意：

- `x-cem` 目前是无 scope 的公共包，默认 `access=public`，不需要 `--access`。
- **同一个版本号不能覆盖发布**。发现产物有问题时只能发新 patch 版本，不要尝试
  `npm unpublish` 后重发（72 小时窗口外也无法删除；优先 `npm deprecate`）。
- 不要把仓库根目录当包根发布；根目录没有 `package.json`，会直接失败或打错包。
- 如需 provenance，可在有 CI OIDC 的环境里用 `npm publish --provenance`；
  本机手动发布不需要。

## 2. crates.io 发布

```sh
# 先 dry-run，确认打包内容
cargo package --list
cargo publish --dry-run

# 真正发布（需要人类先 cargo login，或提供 CARGO_REGISTRY_TOKEN）
cargo publish
```

发布后验证：

```sh
cargo install x-cem --version <version> --force
x-cem --version
```

注意：

- crates.io 同样**不允许覆盖已发布版本**，只能 bump 版本重发。
- `cargo publish --dry-run` 会检查 `Cargo.toml` 的 `readme` / `license` / `repository` 等元数据；
  当前 `README.md` 已存在，且 `Cargo.toml` 的 `readme = "README.md"` 指向它。
- 发布顺序建议：先 crates.io，确认成功后再 npm；两边都失败时不要重复使用同一版本号。

## 3. 版本号规则与同步位置

采用 [SemVer](https://semver.org/lang/zh-CN/)；`0.x` 阶段把 MINOR 当作 breaking 信号：

| 变更类型 | 版本位 |
| --- | --- |
| bug 修复、文档、内部重构，输出不变 | PATCH |
| 新增可识别的语法 / 新字段，向后兼容 | MINOR |
| CLI 参数、输出结构、ABI 破坏性变更 | MINOR（`0.x` 阶段）或 MAJOR（`1.0.0` 后） |

必须同步修改的地方：

1. `Cargo.toml` 的 `[package].version`
2. `npm/package.json` 的 `version`
3. `CHANGELOG.md`（仓库目前还没有，首次发布前建一个，至少记录本次版本）
4. `Cargo.lock`（`cargo build` / `cargo publish` 会自动更新，提交时一起带上）
5. `README.md` 中出现的版本号、性能数字（若基准或实现有变化，重跑后更新）

不需要手动改的地方：原生 CLI 的 `--version` 来自 `CARGO_PKG_VERSION`；
npm CLI 的 `--version` 读取 `npm/package.json`。

## 4. 发布门禁（必须遵守）

| 动作 | 谁执行 | 门禁 |
| --- | --- | --- |
| 修改版本号、更新 CHANGELOG | 人 / agent 均可 | 需要人类确认目标版本 |
| `npm pack --dry-run`、`cargo publish --dry-run` | agent 可执行 | 只读检查，不产生发布 |
| `npm publish` | **仅人类** | 执行前明确确认版本与 registry |
| `cargo publish` | **仅人类** | 执行前明确确认版本 |
| `git tag` / `git push` | **仅人类** | 发布验证通过后由人类打 tag |
| 发布后改写已发布的版本 | **禁止** | 只能发新版本 |

如果任务要求 agent 自动发布，应拒绝并回到人工确认流程。

## 5. 发布后

1. 在两个 registry 上确认版本可安装（见第 1、2 节的验证命令）。
2. 人类打 tag：`git tag v<version> && git push origin v<version>`（agent 不代做）。
3. 在 GitHub Releases 写变更摘要，附上 `CHANGELOG.md` 对应段落。
4. 若发布有误：npm 用 `npm deprecate x-cem@<version> "原因"`；crates.io 只能 yank
   （`cargo yank --version <version>`），随后尽快发修复版本。

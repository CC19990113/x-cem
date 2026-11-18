/**
 * wasm 加载器：只做「读字节 → WebAssembly.instantiate」。
 *
 * 目标平台是 wasm32-unknown-unknown 的纯 wasm，没有 WASI、也没有任何宿主导入，
 * 因此这里不引入 `node:wasi`，实例化时传入空导入对象。
 *
 * 文件名容错：优先 `core.wasm`，其次 `x_cem.wasm`（Cargo 的 cdylib 产物名）。
 * 两者都不在时给出可操作的错误，而不是让 Node 抛裸 ENOENT。
 */

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

/** 候选文件名，按优先级排列。 */
const WASM_CANDIDATES = ['core.wasm', 'x_cem.wasm'];

/** wasm ABI 必须导出的符号（见仓库根 README 的 ABI 契约）。 */
const REQUIRED_EXPORTS = [
  'memory',
  'xcem_alloc',
  'xcem_free',
  'xcem_scan',
  'xcem_result_ptr',
  'xcem_result_len',
];

const HERE = fileURLToPath(new URL('./', import.meta.url));

/** 模块级缓存：同一个进程只实例化一次。失败不缓存，修复产物后可重试。 */
let instancePromise = null;

/**
 * 加载并实例化 wasm。
 *
 * @returns {Promise<WebAssembly.Instance>}
 */
export function loadWasm() {
  if (!instancePromise) {
    instancePromise = instantiateWasm().catch((error) => {
      instancePromise = null;
      throw error;
    });
  }
  return instancePromise;
}

async function instantiateWasm() {
  const found = await readFirstExisting();

  let instance;
  try {
    // 空导入对象：纯 wasm 没有导入；若产物误用 WASI 会在这里抛出可定位的错误。
    ({ instance } = await WebAssembly.instantiate(found.bytes, {}));
  } catch (error) {
    throw new Error(
      `WebAssembly 实例化失败（${found.name}）：${error.message}\n` +
        '请确认该文件由 wasm32-unknown-unknown 目标构建（不是 WASI），且 Node.js >= 18。',
      { cause: error },
    );
  }

  const missing = REQUIRED_EXPORTS.filter((name) => !(name in instance.exports));
  if (missing.length > 0) {
    throw new Error(
      `wasm 产物 ${found.name} 缺少 ABI 导出：${missing.join(', ')}。\n` +
        '这个文件可能不是 x-cem 的 wasm 产物，或 Rust 侧 ABI 版本不匹配。',
    );
  }

  return instance;
}

async function readFirstExisting() {
  const missing = [];

  for (const name of WASM_CANDIDATES) {
    try {
      const bytes = await readFile(new URL(name, import.meta.url));
      return { name, bytes };
    } catch (error) {
      if (error && error.code === 'ENOENT') {
        missing.push(name);
        continue;
      }
      throw new Error(
        `读取 wasm 产物失败（${name}）：${error.message}\n查找目录：${HERE}`,
        { cause: error },
      );
    }
  }

  throw new Error(
    [
      `找不到 wasm 产物：在 ${HERE} 下既没有 core.wasm，也没有 x_cem.wasm。`,
      '请先在仓库根目录运行 `pnpm build:wasm`（若仓库未提供该脚本，手动执行）：',
      '  cargo build --release --target wasm32-unknown-unknown',
      '  cp target/wasm32-unknown-unknown/release/x_cem.wasm npm/core.wasm',
      `（本次查找过：${missing.join('、')}）`,
    ].join('\n'),
  );
}

/**
 * x-cem 的 Node API。
 *
 * 分工：Node 侧负责读文件、遍历目录、拼装 CEM 根对象；解析全部在 wasm 里完成。
 * Rust 侧 ABI 是「单文件」的（xcem_scan(path, src)），因此：
 * - `scanSource` 一次只处理一个源文件；
 * - `scanDir` 逐文件调用，再按 CEM 2.1.0 结构拼装。
 *
 * 跨文件 `customElements.define(tag, Class)` 的归属本应由 Rust 侧在目录级扫描时完成。
 * 如果 Rust 侧返回的单文件 module 尚未回填 `tagName`，下面的 `backfillTagNames`
 * 会做一次保守兜底：用轻量词法收集全部文件的 define，再按「同文件优先 / 跨文件唯一才分配」
 * 的规则回填。兜底不覆盖 Rust 已经给出的 tagName；一旦 Rust 侧在 wasm 内完成关联，
 * 这段逻辑自然变成 no-op。
 */

import { readFile, readdir, realpath, stat } from 'node:fs/promises';
import path from 'node:path';

import { loadWasm } from './wasm-loader.js';

export const SCHEMA_VERSION = '2.1.0';

const SOURCE_EXTENSIONS = new Set(['.ts', '.js']);
const SKIP_DIR_NAMES = new Set(['node_modules', 'target', 'dist', 'coverage']);

const utf8Decoder = new TextDecoder('utf-8', { fatal: true });

/**
 * 解析单个源文件，返回一个 CEM module 对象。
 *
 * @param {{ path: string, source: string }} input
 *   `path` 只用于 module.path / 错误信息；`source` 是 UTF-8 源码文本。
 * @returns {Promise<object>} module 对象，形如 `{ kind: 'javascript-module', path, declarations }`
 */
export async function scanSource({ path: sourcePath, source } = {}) {
  if (typeof sourcePath !== 'string' || sourcePath.length === 0) {
    throw new TypeError('scanSource: `path` 必须是非空字符串');
  }
  if (typeof source !== 'string') {
    throw new TypeError('scanSource: `source` 必须是字符串');
  }

  const instance = await loadWasm();
  const json = callScan(instance, sourcePath, source);

  let parsed;
  try {
    parsed = JSON.parse(json);
  } catch (error) {
    throw new Error(
      `wasm 返回了非法 JSON（${sourcePath}）：${error.message}`,
      { cause: error },
    );
  }

  return normalizeModule(parsed, sourcePath);
}

/**
 * 递归扫描目录（也接受单个文件），返回 CEM 根对象。
 *
 * @param {string} dir
 * @returns {Promise<{ schemaVersion: string, modules: object[] }>}
 */
export async function scanDir(dir) {
  const { manifest } = await scanDirWithWarnings(dir);
  return manifest;
}

/**
 * 与 {@link scanDir} 相同，但额外返回关联阶段的 warning 文本。
 * CLI 用它把警告打到 stderr，与 Rust 原生 CLI 行为一致。
 *
 * @param {string} dir
 * @returns {Promise<{ manifest: { schemaVersion: string, modules: object[] }, warnings: string[] }>}
 */
export async function scanDirWithWarnings(dir) {
  if (typeof dir !== 'string' || dir.length === 0) {
    throw new TypeError('scanDir: `dir` 必须是非空字符串');
  }

  const input = path.resolve(dir);
  let root;
  try {
    root = await realpath(input);
  } catch (error) {
    const detail = error && error.code === 'ENOENT' ? '路径不存在' : error.message;
    throw new Error(`无法访问 ${dir}：${detail}`, { cause: error });
  }

  let rootStat;
  try {
    rootStat = await stat(root);
  } catch (error) {
    throw new Error(`无法访问 ${dir}：${error.message}`, { cause: error });
  }
  const files = [];

  if (rootStat.isFile()) {
    files.push({
      absolutePath: root,
      displayPath: displayPath(root, path.dirname(root)),
    });
  } else if (rootStat.isDirectory()) {
    await walk(root, files, root);
  } else {
    throw new Error(`无法访问 ${dir}：既不是普通文件也不是目录`);
  }

  const scanned = [];
  const included = [];

  files.sort((a, b) =>
    a.absolutePath < b.absolutePath ? -1 : a.absolutePath > b.absolutePath ? 1 : 0,
  );

  for (const file of files) {
    let buffer;
    try {
      buffer = await readFile(file.absolutePath);
    } catch (error) {
      throw new Error(`读取 ${file.absolutePath} 失败：${error.message}`, { cause: error });
    }

    let source;
    try {
      source = utf8Decoder.decode(buffer);
    } catch {
      throw new Error(`${file.displayPath} 不是合法 UTF-8；x-cem 只处理 UTF-8 源文件`);
    }

    const module = await scanSource({ path: file.displayPath, source });
    const fileIndex = scanned.length;
    scanned.push({ source });

    if (module.declarations.length > 0) {
      included.push({ module, fileIndex });
    }
  }

  // 兜底：Rust 单文件 ABI 不会做跨文件关联时，在 JS 侧补 tagName。
  // 所有声明都已有 tagName 时直接跳过，避免再做一遍 JS 词法扫描。
  const needsBackfill = included.some(({ module }) =>
    module.declarations.some((declaration) => !declaration.tagName),
  );
  const warnings = needsBackfill ? backfillTagNames(included, scanned) : [];

  const manifest = {
    schemaVersion: SCHEMA_VERSION,
    modules: included.map(({ module }) => module),
  };

  return { manifest, warnings };
}

/* ------------------------------------------------------------------ */
/* wasm 调用                                                            */
/* ------------------------------------------------------------------ */

const textEncoder = new TextEncoder();

function callScan(instance, sourcePath, source) {
  const { exports } = instance;
  const memory = exports.memory;

  const pathBytes = textEncoder.encode(sourcePath);
  const sourceBytes = textEncoder.encode(source);

  // wasm 侧的 alloc(0) 行为未约定；统一至少分配 1 字节，free 时用同一个长度。
  const pathLength = Math.max(1, pathBytes.length);
  const sourceLength = Math.max(1, sourceBytes.length);

  const pathPtr = exports.xcem_alloc(pathLength);
  if (!pathPtr) {
    throw new Error('wasm 内存分配失败：xcem_alloc 返回空指针');
  }
  const sourcePtr = exports.xcem_alloc(sourceLength);
  if (!sourcePtr) {
    exports.xcem_free(pathPtr, pathLength);
    throw new Error('wasm 内存分配失败：xcem_alloc 返回空指针');
  }

  let resultBytes;
  try {
    // 每次取视图都必须基于最新的 memory.buffer：alloc 可能触发内存增长。
    new Uint8Array(memory.buffer, pathPtr, pathBytes.length).set(pathBytes);
    new Uint8Array(memory.buffer, sourcePtr, sourceBytes.length).set(sourceBytes);

    const code = exports.xcem_scan(pathPtr, pathBytes.length, sourcePtr, sourceBytes.length);
    if (code !== 0) {
      const detail = readLastError(exports);
      throw new Error(
        `解析失败（${sourcePath}）：wasm xcem_scan 返回错误码 ${code}${detail ? `（${detail}）` : ''}`,
      );
    }

    const resultPtr = exports.xcem_result_ptr();
    const resultLength = exports.xcem_result_len();
    if (resultLength > 0 && !resultPtr) {
      throw new Error(`解析失败（${sourcePath}）：xcem_result_ptr 返回空指针但长度为 ${resultLength}`);
    }

    // 先拷贝出 wasm 内存，再 free 输入缓冲。
    resultBytes =
      resultLength > 0
        ? new Uint8Array(memory.buffer, resultPtr, resultLength).slice()
        : new Uint8Array(0);
  } finally {
    exports.xcem_free(pathPtr, pathLength);
    exports.xcem_free(sourcePtr, sourceLength);
  }

  try {
    return utf8Decoder.decode(resultBytes);
  } catch (error) {
    throw new Error(
      `解析失败（${sourcePath}）：wasm 返回的结果不是合法 UTF-8：${error.message}`,
      { cause: error },
    );
  }
}

/**
 * 可选的 ABI 扩展：Rust 侧额外导出了 xcem_last_error_ptr / len。
 * 没有这两个导出时退化为只有错误码，不影响主流程。
 */
function readLastError(exports) {
  if (
    typeof exports.xcem_last_error_ptr !== 'function' ||
    typeof exports.xcem_last_error_len !== 'function'
  ) {
    return '';
  }

  const ptr = exports.xcem_last_error_ptr();
  const length = exports.xcem_last_error_len();
  if (!ptr || !length) {
    return '';
  }

  try {
    const bytes = new Uint8Array(exports.memory.buffer, ptr, length).slice();
    return utf8Decoder.decode(bytes);
  } catch {
    return '';
  }
}

/**
 * 兼容两种 wasm 返回形态：
 * 1. 完整 CEM 根对象 `{ schemaVersion, modules: [...] }`（单文件时取 modules[0]）；
 * 2. 裸 module 对象 `{ kind: 'javascript-module', path, declarations }`。
 */
function normalizeModule(parsed, sourcePath) {
  if (parsed && Array.isArray(parsed.modules)) {
    const module = parsed.modules[0];
    if (!module) {
      return { kind: 'javascript-module', path: sourcePath, declarations: [] };
    }
    return normalizeShape(module, sourcePath);
  }

  if (parsed && Array.isArray(parsed.declarations)) {
    return normalizeShape(parsed, sourcePath);
  }

  throw new Error(
    `wasm 返回结构无法识别（${sourcePath}）：既不是 CEM 根对象（modules 数组），也不是 module 对象（declarations 数组）`,
  );
}

function normalizeShape(module, fallbackPath) {
  return {
    ...module,
    path: typeof module.path === 'string' && module.path.length > 0 ? module.path : fallbackPath,
    declarations: module.declarations,
  };
}

/* ------------------------------------------------------------------ */
/* 目录遍历                                                             */
/* ------------------------------------------------------------------ */

async function walk(dir, files, root) {
  const entries = await readdir(dir, { withFileTypes: true });
  entries.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));

  for (const entry of entries) {
    const absolutePath = path.join(dir, entry.name);

    // 与 Rust 侧一致：不跟随符号链接。
    if (entry.isSymbolicLink()) {
      continue;
    }

    if (entry.isDirectory()) {
      if (entry.name.startsWith('.') || SKIP_DIR_NAMES.has(entry.name)) {
        continue;
      }
      await walk(absolutePath, files, root);
      continue;
    }

    if (!entry.isFile()) {
      continue;
    }

    if (!SOURCE_EXTENSIONS.has(path.extname(entry.name))) {
      continue;
    }

    files.push({ absolutePath, displayPath: displayPath(absolutePath, root) });
  }
}

/**
 * 优先输出相对 cwd 的路径；src 在 cwd 之外时退回相对 src 的路径。
 * 与 Rust CLI 的 display_path 规则一致，统一用 `/`。
 */
function displayPath(file, root) {
  const relativeToCwd = path.relative(process.cwd(), file);
  if (relativeToCwd && !relativeToCwd.startsWith('..') && !path.isAbsolute(relativeToCwd)) {
    return toPosix(relativeToCwd);
  }

  const relativeToRoot = path.relative(root, file);
  if (relativeToRoot && !relativeToRoot.startsWith('..') && !path.isAbsolute(relativeToRoot)) {
    return toPosix(relativeToRoot);
  }

  return toPosix(file);
}

function toPosix(value) {
  return value.split(path.sep).join('/');
}

/* ------------------------------------------------------------------ */
/* 跨文件 tagName 兜底                                                   */
/* ------------------------------------------------------------------ */

/**
 * 从所有已读源码里收集 `customElements.define('tag', Class)`，再给缺失 tagName 的
 * 声明回填。规则与 Rust 文档一致：
 * - 同文件 define 优先；同文件多个不同 tag 视为冲突，不分配；
 * - 否则看跨文件 define：唯一 tag 且该 class 只在一个模块中声明，才分配；
 * - 其余歧义场景只记 warning，不分配。
 *
 * 注意：这是「单文件 ABI」下的兜底，不是完整模块图消歧；Rust 侧若已在目录级完成
 * 关联，这里不会改动任何已有 tagName。
 *
 * @returns {string[]} warnings
 */
function backfillTagNames(included, scanned) {
  const warnings = [];

  const definesByClass = new Map();
  for (let fileIndex = 0; fileIndex < scanned.length; fileIndex += 1) {
    for (const define of findDefines(scanned[fileIndex].source)) {
      if (!definesByClass.has(define.className)) {
        definesByClass.set(define.className, []);
      }
      definesByClass.get(define.className).push({ ...define, fileIndex });
    }
  }

  if (definesByClass.size === 0) {
    return warnings;
  }

  // 这个 class 名出现在哪些「有声明的模块」里。
  const classModules = new Map();
  for (const { module, fileIndex } of included) {
    for (const declaration of module.declarations) {
      if (!classModules.has(declaration.name)) {
        classModules.set(declaration.name, new Set());
      }
      classModules.get(declaration.name).add(fileIndex);
    }
  }

  for (const { module, fileIndex } of included) {
    for (let index = 0; index < module.declarations.length; index += 1) {
      const declaration = module.declarations[index];
      if (declaration.tagName) {
        continue;
      }

      const candidates = definesByClass.get(declaration.name);
      if (!candidates || candidates.length === 0) {
        continue;
      }

      const sameFile = candidates.filter((candidate) => candidate.fileIndex === fileIndex);
      if (sameFile.length > 0) {
        const tags = distinctTags(sameFile);
        if (tags.length === 1) {
          assignTagName(module, index, tags[0]);
        } else {
          warnings.push(
            `类 ${declaration.name} 在 ${module.path} 内有多个不同的 define（${tags.join('、')}），未分配 tagName`,
          );
        }
        continue;
      }

      const tags = distinctTags(candidates);
      const declaringModules = classModules.get(declaration.name);

      if (tags.length > 1) {
        warnings.push(
          `类 ${declaration.name} 被多个 define 指向（${tags.join('、')}），跨文件关联有歧义，未分配 tagName`,
        );
      } else if (!declaringModules || declaringModules.size > 1) {
        warnings.push(
          `类名 ${declaration.name} 在多个模块中出现，跨文件 define 无法确定归属，未分配 tagName`,
        );
      } else {
        assignTagName(module, index, tags[0]);
      }
    }
  }

  return warnings;
}

/**
 * 给声明补 tagName / customElement，并把它们插到 `name` 之后，
 * 让 JS 兜底的输出字段顺序与 Rust 序列化一致。
 */
function assignTagName(module, index, tag) {
  const declaration = module.declarations[index];
  const ordered = {};

  for (const [key, value] of Object.entries(declaration)) {
    if (key === 'tagName' || key === 'customElement') {
      continue;
    }
    ordered[key] = value;
    if (key === 'name') {
      ordered.tagName = tag;
      ordered.customElement = true;
    }
  }

  if (!('tagName' in ordered)) {
    ordered.tagName = tag;
    ordered.customElement = true;
  }

  module.declarations[index] = ordered;
}

function distinctTags(candidates) {
  const tags = [];
  for (const candidate of candidates) {
    if (!tags.includes(candidate.tag)) {
      tags.push(candidate.tag);
    }
  }
  return tags;
}

/**
 * 极简 JS 词法扫描，只服务 define 兜底：
 * 跳过注释与模板字符串，产出 ident / string / 单字符 punct。
 * 它不是完整词法器，正则字面量等边界情况可能误判 —— 兜底只在 Rust 未回填时生效。
 */
function findDefines(source) {
  const tokens = tokenizeLite(source);
  const defines = [];

  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (token.type !== 'ident' || token.value !== 'customElements') {
      continue;
    }

    const parsed = parseDefine(tokens, i);
    if (parsed) {
      defines.push(parsed);
    }
  }

  return defines;
}

function parseDefine(tokens, index) {
  if (!isPunct(tokens[index + 1], '.')) return null;
  if (!isIdent(tokens[index + 2], 'define')) return null;
  if (!isPunct(tokens[index + 3], '(')) return null;

  const tagToken = tokens[index + 4];
  if (!tagToken || tagToken.type !== 'string') return null;
  const tag = unescapeJs(tagToken.value);
  if (!isValidTagName(tag)) return null;
  if (!isPunct(tokens[index + 5], ',')) return null;

  const first = tokens[index + 6];
  if (!first || first.type !== 'ident') return null;

  let className = first.value;
  let cursor = index + 7;
  while (isPunct(tokens[cursor], '.')) {
    const next = tokens[cursor + 1];
    if (!next || next.type !== 'ident') return null;
    className = next.value;
    cursor += 2;
  }

  return { tag, className };
}

function isIdent(token, value) {
  return Boolean(token) && token.type === 'ident' && token.value === value;
}

function isPunct(token, value) {
  return Boolean(token) && token.type === 'punct' && token.value === value;
}

function isValidTagName(tag) {
  return /^[a-z][a-z0-9_.:-]*-[a-z0-9_.:-]*$/.test(tag);
}

const SIMPLE_ESCAPES = { n: '\n', r: '\r', t: '\t', b: '\b', f: '\f', v: '\v', 0: '\0' };

function unescapeJs(value) {
  if (!value.includes('\\')) return value;

  let out = '';
  for (let i = 0; i < value.length; i += 1) {
    const char = value[i];
    if (char !== '\\') {
      out += char;
      continue;
    }
    const next = value[i + 1];
    if (next === undefined) {
      out += '\\';
      break;
    }
    if (next === 'x' && /^[0-9a-fA-F]{2}$/.test(value.slice(i + 2, i + 4))) {
      out += String.fromCharCode(Number.parseInt(value.slice(i + 2, i + 4), 16));
      i += 3;
      continue;
    }
    if (next === 'u') {
      const braced = /^\{([0-9a-fA-F]{1,6})\}/.exec(value.slice(i + 2));
      if (braced) {
        out += String.fromCodePoint(Number.parseInt(braced[1], 16));
        i += 2 + braced[0].length;
        continue;
      }
      if (/^[0-9a-fA-F]{4}$/.test(value.slice(i + 2, i + 6))) {
        out += String.fromCharCode(Number.parseInt(value.slice(i + 2, i + 6), 16));
        i += 5;
        continue;
      }
    }
    out += SIMPLE_ESCAPES[next] ?? next;
    i += 1;
  }
  return out;
}

function tokenizeLite(source) {
  const tokens = [];
  const length = source.length;
  let i = 0;

  const isIdentStart = (char) => /[A-Za-z_$]/.test(char);
  const isIdentPart = (char) => /[A-Za-z0-9_$]/.test(char);

  while (i < length) {
    const char = source[i];

    if (char === '/' && source[i + 1] === '/') {
      const newline = source.indexOf('\n', i + 2);
      i = newline === -1 ? length : newline + 1;
      continue;
    }

    if (char === '/' && source[i + 1] === '*') {
      const end = source.indexOf('*/', i + 2);
      i = end === -1 ? length : end + 2;
      continue;
    }

    if (char === '"' || char === "'" || char === '`') {
      const quote = char;
      let value = '';
      i += 1;
      while (i < length) {
        const current = source[i];
        if (current === '\\') {
          value += current + (source[i + 1] ?? '');
          i += 2;
          continue;
        }
        if (current === quote) {
          i += 1;
          break;
        }
        if (quote !== '`' && current === '\n') {
          break; // 未闭合的普通字符串，就地结束
        }
        value += current;
        i += 1;
      }
      tokens.push({ type: 'string', value });
      continue;
    }

    if (isIdentStart(char)) {
      let end = i + 1;
      while (end < length && isIdentPart(source[end])) {
        end += 1;
      }
      tokens.push({ type: 'ident', value: source.slice(i, end) });
      i = end;
      continue;
    }

    if (/[0-9]/.test(char)) {
      let end = i + 1;
      while (end < length && /[0-9a-fA-F_xXobOBn.]/.test(source[end])) {
        end += 1;
      }
      tokens.push({ type: 'number', value: source.slice(i, end) });
      i = end;
      continue;
    }

    if (!/\s/.test(char)) {
      tokens.push({ type: 'punct', value: char });
    }
    i += 1;
  }

  return tokens;
}

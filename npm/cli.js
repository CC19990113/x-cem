#!/usr/bin/env node
/**
 * x-cem CLI（npm / wasm 版）。
 *
 * 参数与 Rust 原生 CLI 保持一致（手写解析，零依赖）：
 *   x-cem scan --src <目录或文件> [--out <输出文件>]
 *
 * 与原生 CLI 的差异只有一处：`--out` 可省略，缺省或 `-` 时输出到 stdout。
 * 退出码：0 成功；1 扫描/解析失败；2 用法错误。
 */

import { writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';

import { scanDirWithWarnings } from './index.js';

const require = createRequire(import.meta.url);
const pkg = require('./package.json');

const USAGE = `x-cem — 词法级 TS/JS Custom Elements Manifest 生成器（npm / wasm 版）

用法:
  x-cem scan --src <目录或文件> [--out <输出文件>]

选项:
  --src <path>    递归扫描 *.ts / *.js
  --out <path>    输出 CEM JSON；缺省或 \`-\` 表示 stdout
  -h, --help      显示帮助
  -V, --version   显示版本
`;

function printUsage() {
  process.stdout.write(USAGE);
}

function usageError(message) {
  process.stderr.write(`x-cem: ${message}\n\n${USAGE}`);
  return 2;
}

async function runScan(args) {
  let src;
  let out;

  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    const equals = arg.indexOf('=');
    const key = equals === -1 ? arg : arg.slice(0, equals);
    const inlineValue = equals === -1 ? undefined : arg.slice(equals + 1);

    if (key === '-h' || key === '--help') {
      printUsage();
      return 0;
    }

    if (key === '--src' || key === '--out') {
      let value = inlineValue;
      if (value === undefined) {
        i += 1;
        value = args[i];
        if (value === undefined) {
          return usageError(`\`${key}\` 缺少取值`);
        }
      }

      if (key === '--src') {
        src = value;
      } else {
        out = value;
      }
      continue;
    }

    return usageError(`未知参数 \`${key}\``);
  }

  if (src === undefined) {
    return usageError('缺少 `--src`');
  }

  let manifest;
  let warnings;
  try {
    ({ manifest, warnings } = await scanDirWithWarnings(src));
  } catch (error) {
    process.stderr.write(`x-cem: 扫描 \`${src}\` 失败: ${error.message}\n`);
    return 1;
  }

  for (const warning of warnings) {
    process.stderr.write(`x-cem: 警告: ${warning}\n`);
  }

  const json = `${JSON.stringify(manifest, null, 2)}\n`;
  const useStdout = out === undefined || out === '-';

  if (useStdout) {
    process.stdout.write(json);
  } else {
    try {
      await writeFile(out, json, 'utf8');
    } catch (error) {
      process.stderr.write(`x-cem: 写入 \`${out}\` 失败: ${error.message}\n`);
      return 1;
    }
  }

  const declarationCount = manifest.modules.reduce(
    (sum, module) => sum + module.declarations.length,
    0,
  );
  process.stderr.write(
    `x-cem: ${manifest.modules.length} 个模块 / ${declarationCount} 个声明 -> ${
      useStdout ? '-' : out
    }\n`,
  );

  return 0;
}

async function main(argv) {
  const [command, ...rest] = argv;

  if (command === undefined) {
    return usageError('缺少子命令');
  }

  switch (command) {
    case '-h':
    case '--help':
      printUsage();
      return 0;
    case '-V':
    case '--version':
      process.stdout.write(`x-cem ${pkg.version}\n`);
      return 0;
    case 'scan':
      return runScan(rest);
    default:
      return usageError(`未知子命令 \`${command}\``);
  }
}

main(process.argv.slice(2))
  .then((code) => {
    process.exitCode = code;
  })
  .catch((error) => {
    process.stderr.write(`x-cem: ${error && error.stack ? error.stack : error}\n`);
    process.exitCode = 1;
  });

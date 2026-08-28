import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(root, 'pkg');

function run(command, args) {
  execFileSync(command, args, { cwd: root, stdio: 'inherit', env: buildEnv });
}

const buildEnv = { ...process.env };
// zstd-sys is a C dependency. Keep the harness self-contained on Windows by
// using an already-installed LLVM toolchain when it is not on PATH; do not
// install or modify system software as part of a campaign.
if (process.platform === 'win32') {
  const llvmBin = 'C:\\Program Files\\LLVM\\bin';
  const clang = `${llvmBin}\\clang.exe`;
  const llvmAr = `${llvmBin}\\llvm-ar.exe`;
  if (existsSync(clang)) {
    buildEnv.CC_wasm32_unknown_unknown ??= clang;
    buildEnv['CC_wasm32-unknown-unknown'] ??= clang;
  }
  if (existsSync(llvmAr)) {
    buildEnv.AR_wasm32_unknown_unknown ??= llvmAr;
    buildEnv['AR_wasm32-unknown-unknown'] ??= llvmAr;
  }
}

try {
  execFileSync('wasm-pack', ['--version'], { cwd: root, stdio: 'inherit' });
  const installed = execFileSync('rustup', ['target', 'list', '--installed'], {
    cwd: root,
    encoding: 'utf8',
  });
  if (!installed.split(/\r?\n/).includes('wasm32-unknown-unknown')) {
    throw new Error('wasm32-unknown-unknown is not installed');
  }
} catch (error) {
  console.error(`WASM prerequisites unavailable: ${error.message}`);
  process.exit(1);
}

run('wasm-pack', [
  'build',
  path.join(root, 'wasm'),
  '--target',
  'nodejs',
  '--out-dir',
  outDir,
  '--out-name',
  'package_codecs_wasm',
]);

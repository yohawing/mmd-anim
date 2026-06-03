import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const crateDir = path.resolve(__dirname, '..');
const outDir = path.join('harness', 'pkg');

function checkVersion(command, args, label) {
  try {
    const out = execFileSync(command, args, { encoding: 'utf8', stdio: 'pipe' }).trim();
    console.log(`  ${label}: ${out}`);
    return true;
  } catch {
    return false;
  }
}

function hasWasmTarget() {
  try {
    const out = execFileSync('rustup', ['target', 'list', '--installed'], {
      encoding: 'utf8',
      stdio: 'pipe',
    });
    return out.split(/\r?\n/).includes('wasm32-unknown-unknown');
  } catch {
    return false;
  }
}

console.log('Checking prerequisites...');

if (!checkVersion('wasm-pack', ['--version'], 'wasm-pack')) {
  console.error('\nERROR: wasm-pack is not installed.');
  console.error('Install it with one of:');
  console.error('  npm install -g wasm-pack');
  console.error('  cargo install wasm-pack');
  process.exit(1);
}

if (!hasWasmTarget()) {
  console.error('\nERROR: wasm32-unknown-unknown target is not installed.');
  console.error('Install it with:');
  console.error('  rustup target add wasm32-unknown-unknown');
  process.exit(1);
}
console.log('  wasm32 target: wasm32-unknown-unknown');

console.log('\nBuilding wasm crate with wasm-pack for web...');
execFileSync('wasm-pack', ['build', '.', '--target', 'web', '--out-dir', outDir], {
  cwd: crateDir,
  stdio: 'inherit',
});
console.log('\nBuild complete. Output in harness/pkg/');

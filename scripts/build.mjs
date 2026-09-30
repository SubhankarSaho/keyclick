// Release builds. Usage: node scripts/build.mjs <windows|macos|all>
//   windows -> release/KeyClick-Setup.exe (+ release/KeyClick.msi)
//   macos   -> release/KeyClick.dmg + release/KeyClick.app (universal: Intel + Apple Silicon)
// Native installers must be built on their own OS; "all" builds what the host supports.
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { root, tauri } from './lib.mjs';

const which = process.argv[2] ?? 'all';
const targetDir = process.env.CARGO_TARGET_DIR || join(root, 'target');
const releaseDir = join(root, 'release');

function newest(dir, ext) {
  if (!existsSync(dir)) return null;
  const files = readdirSync(dir).filter((f) => f.endsWith(ext)).sort();
  return files.length ? join(dir, files[files.length - 1]) : null;
}

function publish(from, name) {
  if (!from) {
    console.error(`Expected build output for ${name} was not found.`);
    process.exit(1);
  }
  const to = join(releaseDir, name);
  rmSync(to, { recursive: true, force: true });
  cpSync(from, to, { recursive: true });
  console.log(`  ${name}  <-  ${from}`);
}

function buildWindows() {
  if (process.platform !== 'win32') {
    console.error('build-windows must run on Windows (NSIS/MSI bundling is Windows-native).');
    process.exit(1);
  }
  // KEYCLICK_BUNDLES=nsis skips the MSI (e.g. when WiX cannot be downloaded).
  const bundles = process.env.KEYCLICK_BUNDLES || 'nsis,msi';
  tauri(['build', '--bundles', bundles]);
  const bundle = join(targetDir, 'release', 'bundle');
  mkdirSync(releaseDir, { recursive: true });
  console.log('\nRelease files:');
  publish(newest(join(bundle, 'nsis'), '-setup.exe'), 'KeyClick-Setup.exe');
  if (bundles.includes('msi')) publish(newest(join(bundle, 'msi'), '.msi'), 'KeyClick.msi');
}

function buildMacos() {
  if (process.platform !== 'darwin') {
    console.error('build-macos must run on macOS with Xcode command line tools installed.');
    process.exit(1);
  }
  const target = process.env.KEYCLICK_MAC_TARGET || 'universal-apple-darwin';
  tauri(['build', '--target', target, '--bundles', 'app,dmg']);
  const bundle = join(targetDir, target, 'release', 'bundle');
  mkdirSync(releaseDir, { recursive: true });
  console.log('\nRelease files:');
  publish(newest(join(bundle, 'dmg'), '.dmg'), 'KeyClick.dmg');
  publish(newest(join(bundle, 'macos'), '.app'), 'KeyClick.app');
}

if (which === 'windows') buildWindows();
else if (which === 'macos') buildMacos();
else if (which === 'all') {
  if (process.platform === 'win32') buildWindows();
  else if (process.platform === 'darwin') buildMacos();
  else {
    console.error('KeyClick installers are built on Windows (exe/msi) and macOS (dmg).');
    process.exit(1);
  }
  console.log('\nNote: the other platform must be built on its own OS (see docs/RELEASE.md).');
} else {
  console.error(`Unknown target "${which}". Use windows, macos or all.`);
  process.exit(1);
}

// Shared helpers for the build scripts. Runs tools directly (no shell) so
// paths with spaces work on every platform.
import { spawnSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = join(dirname(fileURLToPath(import.meta.url)), '..');
export const tauriDir = join(root, 'src-tauri');
const tauriCli = join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');

export function run(cmd, args, opts = {}) {
  console.log(`\n> ${cmd} ${args.join(' ')}`);
  const res = spawnSync(cmd, args, { stdio: 'inherit', cwd: root, ...opts });
  if (res.error) throw res.error;
  if (res.status !== 0) {
    console.error(`\n${cmd} exited with code ${res.status}`);
    process.exit(res.status ?? 1);
  }
}

export function tauri(args, opts) {
  run(process.execPath, [tauriCli, ...args], opts);
}

export function npm(args, opts) {
  // npm-cli.js ships next to the running node binary's npm.
  const npmCli = process.env.npm_execpath;
  if (npmCli && npmCli.endsWith('.js')) run(process.execPath, [npmCli, ...args], opts);
  else run(process.platform === 'win32' ? 'npm.cmd' : 'npm', args, { shell: process.platform === 'win32', ...opts });
}

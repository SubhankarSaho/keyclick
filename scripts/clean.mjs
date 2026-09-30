// Removes build outputs. Sources, generated sounds and icons are kept.
import { rmSync } from 'node:fs';
import { join } from 'node:path';
import { root, run } from './lib.mjs';

for (const dir of ['dist', 'release']) {
  rmSync(join(root, dir), { recursive: true, force: true });
  console.log(`removed ${dir}/`);
}
run('cargo', ['clean', '--manifest-path', join(root, 'src-tauri', 'Cargo.toml')]);

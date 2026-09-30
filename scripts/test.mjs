// Runs every automated check: Rust unit/integration tests and the UI type-check.
import { join } from 'node:path';
import { npm, root, run } from './lib.mjs';

run('cargo', ['test', '--manifest-path', join(root, 'src-tauri', 'Cargo.toml')]);
npm(['run', 'typecheck']);
console.log('\nAll checks passed.');

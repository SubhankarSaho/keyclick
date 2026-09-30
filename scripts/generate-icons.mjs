// Regenerates every app/tray icon from the SVG sources in src-tauri/icons/source.
import { copyFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { tauri, tauriDir } from './lib.mjs';

const icons = join(tauriDir, 'icons');
const source = join(icons, 'source');

tauri(['icon', join(source, 'app-icon.svg'), '-o', icons]);
// Mobile icon sets are not used by this desktop-only app.
for (const dir of ['android', 'ios']) rmSync(join(icons, dir), { recursive: true, force: true });

for (const [svg, out] of [
  ['tray.svg', 'tray.png'],
  ['tray-template.svg', 'tray-template.png'],
]) {
  const tmp = mkdtempSync(join(tmpdir(), 'keyclick-icon-'));
  tauri(['icon', join(source, svg), '-o', tmp, '-p', '64']);
  copyFileSync(join(tmp, '64x64.png'), join(icons, out));
  rmSync(tmp, { recursive: true, force: true });
}
console.log('\nIcons written to', icons);

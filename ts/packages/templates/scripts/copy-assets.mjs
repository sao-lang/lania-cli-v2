import { cpSync, existsSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptsDir = fileURLToPath(new URL('.', import.meta.url));
const packageDir = resolve(scriptsDir, '..');
const assetPairs = [
  ['src/templates', 'dist/templates'],
  ['src/add-templates/assets', 'dist/add-templates/assets'],
];

for (const [sourceRelativeDir, distRelativeDir] of assetPairs) {
  const sourceDir = resolve(packageDir, sourceRelativeDir);
  const distDir = resolve(packageDir, distRelativeDir);
  if (!existsSync(sourceDir)) {
    throw new Error(`asset source directory not found: ${sourceDir}`);
  }
  mkdirSync(distDir, { recursive: true });
  cpSync(sourceDir, distDir, { recursive: true });
}

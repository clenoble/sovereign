#!/usr/bin/env node
/**
 * Build the Sovereign GE frontend (frontend/) regardless of the cwd
 * the calling process spawns us from.
 *
 * Tauri's `beforeBuildCommand` runs from different directories
 * depending on the target:
 *   - Desktop  → crates/sovereign-app/    (where tauri.conf.json lives)
 *   - Android  → crates/                  (one level up — confirmed via npm
 *                                          debug log for the v0.0.5 build)
 *
 * Hard-coding `--prefix ../../frontend` worked on desktop but resolved
 * to a non-existent path during `tauri android build`. This script
 * walks up from process.cwd() until it finds a `frontend/package.json`
 * sibling, then runs `npm run build` there with stdio inherited.
 */
const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

function findFrontendDir(start) {
  let dir = start;
  const root = path.parse(dir).root;
  while (dir !== root) {
    const candidate = path.join(dir, 'frontend', 'package.json');
    if (fs.existsSync(candidate)) {
      return path.dirname(candidate);
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

const frontendDir = findFrontendDir(process.cwd());
if (!frontendDir) {
  console.error(
    `[tauri-build-frontend] could not locate frontend/package.json walking up from ${process.cwd()}`
  );
  process.exit(1);
}

console.log(`[tauri-build-frontend] building in ${frontendDir}`);
try {
  execSync('npm run build', { cwd: frontendDir, stdio: 'inherit' });
} catch (e) {
  console.error(`[tauri-build-frontend] npm run build failed: ${e.message}`);
  process.exit(1);
}

import {
  copyFile,
  mkdir,
  readFile,
  readdir,
  readlink,
  realpath,
  rm,
  symlink,
  writeFile,
} from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';

import { asRecord, fileExists } from '../../core/runtime.js';

// File system primitives for the product plugin.
//
// Design goals:
// - Keep handlers focused on "what" (build/pack/publish) and move IO mechanics here.
// - Preserve behavior exactly (including symlink handling and skip rules).

export async function writeJsonFile(filePath: string, value: unknown): Promise<void> {
  // Always mkdirp because report locations are often nested under `.lania/...`.
  await mkdir(dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

export async function readJsonFile(filePath: string): Promise<Record<string, unknown>> {
  return asRecord(JSON.parse(await readFile(filePath, 'utf8')));
}

export async function readOptionalJsonFile(filePath: string): Promise<Record<string, unknown>> {
  if (!(await fileExists(filePath))) {
    return {};
  }
  return await readJsonFile(filePath);
}

export async function ensureModuleSchemaRoot(schemaRoot: string): Promise<void> {
  // When schema roots are copied into `dist/schema-roots/...`, we want Node ESM resolution to work.
  // The simplest way is to ensure `type: module` exists in that root.
  const packageJsonPath = join(schemaRoot, 'package.json');
  let packageJson: Record<string, unknown> = {};
  if (await fileExists(packageJsonPath)) {
    packageJson = await readJsonFile(packageJsonPath);
  }
  await writeJsonFile(packageJsonPath, {
    ...packageJson,
    type: 'module',
  });
}

export async function copyDirectory(
  sourceDir: string,
  targetDir: string,
  skipPaths: string[] = [],
): Promise<void> {
  await mkdir(targetDir, { recursive: true });
  const entries = await readdir(sourceDir, { withFileTypes: true });
  for (const entry of entries) {
    const sourcePath = join(sourceDir, entry.name);
    if (shouldSkipPath(sourcePath, skipPaths)) {
      continue;
    }
    const targetPath = join(targetDir, entry.name);

    // Directory recursion keeps layout intact.
    if (entry.isDirectory()) {
      await copyDirectory(sourcePath, targetPath, skipPaths);
      continue;
    }

    // Preserve symlinks rather than copying dereferenced contents.
    if (entry.isSymbolicLink()) {
      await mkdir(dirname(targetPath), { recursive: true });
      await symlink(await readlink(sourcePath), targetPath);
      continue;
    }

    // Standard file copy.
    if (entry.isFile()) {
      await mkdir(dirname(targetPath), { recursive: true });
      await copyFile(sourcePath, targetPath);
    }
  }
}

function shouldSkipPath(sourcePath: string, skipPaths: string[]): boolean {
  // This is used to avoid copying `node_modules` when staging dependencies.
  for (const skipPath of skipPaths) {
    if (sourcePath === skipPath || sourcePath.startsWith(`${skipPath}/`)) {
      return true;
    }
  }
  return false;
}

export async function stageRuntimePackage(
  sourcePackageRoot: string,
  packageName: string,
  targetNodeModules: string,
  stagedPackages: Set<string>,
): Promise<void> {
  // Guard against infinite recursion and duplicated work across dependency graphs.
  if (stagedPackages.has(packageName)) {
    return;
  }
  stagedPackages.add(packageName);

  // We locate the dependency relative to the source root, walking up to support hoisted installs.
  const sourcePath = await resolveNodeModulePackage(sourcePackageRoot, packageName);
  const targetPath = join(targetNodeModules, packageName);
  await rm(targetPath, { recursive: true, force: true });
  await mkdir(dirname(targetPath), { recursive: true });

  // Copy the package contents, but never include its nested node_modules.
  await copyDirectory(sourcePath, targetPath, [join(sourcePath, 'node_modules')]);

  // Recursively stage dependencies so the staged node-bridge payload is self-contained.
  const packageJson = await readJsonFile(join(sourcePath, 'package.json'));
  const dependencyNames = Object.keys(asRecord(packageJson.dependencies));
  for (const dependencyName of dependencyNames) {
    await stageRuntimePackage(sourcePath, dependencyName, targetNodeModules, stagedPackages);
  }

  // Optional deps might not exist for the current platform; absence is OK.
  const optionalDependencyNames = Object.keys(asRecord(packageJson.optionalDependencies));
  for (const dependencyName of optionalDependencyNames) {
    try {
      await stageRuntimePackage(sourcePath, dependencyName, targetNodeModules, stagedPackages);
    } catch {
      // Skip platform-specific optional packages that are not present in the current install.
    }
  }
}

async function resolveNodeModulePackage(
  sourcePackageRoot: string,
  packageName: string,
): Promise<string> {
  let currentDir = sourcePackageRoot;
  for (;;) {
    const candidate = resolve(currentDir, 'node_modules', packageName);
    if (await fileExists(candidate)) {
      return await realpath(candidate);
    }
    const parentDir = dirname(currentDir);
    if (parentDir === currentDir) {
      break;
    }
    currentDir = parentDir;
  }
  throw new Error(`Unable to resolve runtime package "${packageName}" from ${sourcePackageRoot}`);
}


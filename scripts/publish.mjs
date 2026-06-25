import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const esmEntrypointCache = new Map();

function run(command, args, options = {}) {
  const resolved = resolveSpawnCommand(command, args);
  const result = spawnSync(resolved.command, resolved.args, {
    stdio: 'inherit',
    cwd: options.cwd ?? repoRoot,
    env: { ...process.env, ...(options.env ?? {}) },
  });
  if (result.status !== 0) {
    throw new Error(`Command failed: ${command} ${args.join(' ')}`);
  }
}

function canRun(command, args = ['--version']) {
  const resolved = resolveSpawnCommand(command, args);
  const result = spawnSync(resolved.command, resolved.args, {
    stdio: 'ignore',
    cwd: repoRoot,
    env: process.env,
  });
  return result.status === 0;
}

function readRootPackageManagerVersion() {
  const pkg = JSON.parse(
    spawnSync('node', ['-p', "JSON.stringify(require('./package.json').packageManager)"], {
      cwd: repoRoot,
      encoding: 'utf8',
      env: process.env,
    }).stdout || '"pnpm@10.25.0"',
  );
  return String(pkg).split('@').slice(1).join('@') || '10.25.0';
}

function runPnpm(args) {
  if (canRun('pnpm')) {
    run('pnpm', args, { cwd: repoRoot });
    return;
  }
  const pinned = readRootPackageManagerVersion();
  run('npm', ['exec', '--yes', `pnpm@${pinned}`, '--', ...args], { cwd: repoRoot });
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function parseArgs(argv) {
  const result = {
    manifest: null,
    dryRun: false,
    otp: null,
    npmBin: null,
    yes: false,
    resume: false,
    maxRetries: 0,
    retryDelayMs: 1_000,
    rollbackOnFailure: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--manifest') {
      result.manifest = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (arg === '--dry-run') {
      result.dryRun = true;
      continue;
    }
    if (arg === '--otp') {
      result.otp = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (arg === '--npm-bin') {
      result.npmBin = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (arg === '--yes') {
      result.yes = true;
      continue;
    }
    if (arg === '--resume') {
      result.resume = true;
      continue;
    }
    if (arg === '--max-retries') {
      result.maxRetries = parseIntegerFlag(argv[index + 1], '--max-retries');
      index += 1;
      continue;
    }
    if (arg === '--retry-delay-ms') {
      result.retryDelayMs = parseIntegerFlag(argv[index + 1], '--retry-delay-ms');
      index += 1;
      continue;
    }
    if (arg === '--rollback-on-failure') {
      result.rollbackOnFailure = true;
    }
  }
  return result;
}

function parseIntegerFlag(rawValue, flagName) {
  const parsed = Number.parseInt(String(rawValue ?? ''), 10);
  if (!Number.isFinite(parsed) || parsed < 0) {
    throw new Error(`${flagName} must be a non-negative integer`);
  }
  return parsed;
}

function createInitialExecutionState(dryRun, completedSteps = [], retryPolicy = { maxRetries: 0, retryDelayMs: 1_000 }) {
  return {
    executed: false,
    dryRun,
    completedSteps: [...completedSteps],
    resumed: completedSteps.length > 0,
    failedStepId: null,
    lastError: null,
    attempts: [],
    retryPolicy,
    rollbackPlan: {
      status: 'not_needed',
      generatedAt: null,
      reason: null,
      commands: [],
    },
    updatedAt: new Date().toISOString(),
  };
}

function writeManifest(manifestPath, manifest) {
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n', 'utf8');
}

function resolveExecutionTarballPath(manifestPath, rawTarball) {
  return path.resolve(path.dirname(manifestPath), rawTarball.replace(/^\.\//, ''));
}

function runForOutput(command, args, options = {}) {
  const resolved = resolveSpawnCommand(command, args);
  return spawnSync(resolved.command, resolved.args, {
    cwd: options.cwd ?? repoRoot,
    encoding: 'utf8',
    env: { ...process.env, ...(options.env ?? {}) },
  });
}

function resolveSpawnCommand(command, args) {
  // The publish script supports injecting a custom `--npm-bin` for tests and CI.
  //
  // Some tests provide a fake `npm` implemented as an extensionless ESM script that
  // starts with a Node shebang and uses `import ...`. Earlier we relied on
  // `--experimental-default-type=module`, but newer Node versions no longer accept
  // that flag. To keep the behavior stable across Node versions, we materialize a
  // temporary `.mjs` copy and execute that file with the current Node binary.
  try {
    if (typeof command !== 'string' || command.length === 0) {
      return { command, args };
    }
    const looksLikePath = command.includes('/') || command.includes('\\') || command.startsWith('.');
    if (!looksLikePath) {
      return { command, args };
    }
    const head = fs.readFileSync(command, 'utf8').slice(0, 4096);
    const firstLine = head.split('\n', 1)[0] ?? '';
    const isNodeShebang = firstLine.startsWith('#!') && firstLine.includes('node');
    if (!isNodeShebang) {
      return { command, args };
    }
    const hasEsmImport = /(^|\n)\s*import\s/m.test(head);
    if (hasEsmImport) {
      return { command: process.execPath, args: [materializeNodeEsmEntrypoint(command, head), ...args] };
    }
    return { command: process.execPath, args: [command, ...args] };
  } catch {
    return { command, args };
  }
}

function materializeNodeEsmEntrypoint(command, source) {
  const stat = fs.statSync(command);
  const cacheKey = `${command}:${stat.size}:${stat.mtimeMs}`;
  const cached = esmEntrypointCache.get(cacheKey);
  if (cached && fs.existsSync(cached)) {
    return cached;
  }
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'lania-publish-esm-'));
  const tempEntrypoint = path.join(tempDir, `${path.basename(command)}.mjs`);
  fs.writeFileSync(tempEntrypoint, source, 'utf8');
  esmEntrypointCache.set(cacheKey, tempEntrypoint);
  return tempEntrypoint;
}

function runPublishPreflight(manifestPath, manifest, options) {
  if (!options.dryRun && !options.yes) {
    throw new Error('Real publish execution requires --yes to avoid accidental registry pushes');
  }
  let tarballsVerified = 0;
  for (const step of manifest.steps) {
    const tarballPath = resolveExecutionTarballPath(manifestPath, step.tarball ?? step.command.args[1]);
    if (!fs.existsSync(tarballPath)) {
      throw new Error(`Publish preflight missing tarball for ${step.packageName}: ${step.tarball ?? step.command.args[1]}`);
    }
    tarballsVerified += 1;
  }
  const registry = manifest.steps[0]?.command?.args?.includes('--registry')
    ? manifest.steps[0].command.args[manifest.steps[0].command.args.indexOf('--registry') + 1]
    : 'https://registry.npmjs.org/';
  const whoami = runForOutput(options.npmBin ?? 'npm', ['whoami', '--registry', registry], {
    cwd: path.dirname(manifestPath),
  });
  if (whoami.status !== 0) {
    throw new Error(`Publish preflight npm whoami failed for ${registry}: ${whoami.stderr || whoami.stdout || 'unknown error'}`);
  }
  const actor = whoami.stdout.trim() || null;
  const versionConflicts = [];
  const packageByName = new Map((manifest.packages ?? []).map((entry) => [entry.name, entry]));
  for (const step of manifest.steps) {
    const pkg = packageByName.get(step.packageName);
    if (!pkg?.version) {
      continue;
    }
    const stepRegistry = step.command.args.includes('--registry')
      ? step.command.args[step.command.args.indexOf('--registry') + 1]
      : registry;
    const view = runForOutput(
      options.npmBin ?? 'npm',
      ['view', `${step.packageName}@${pkg.version}`, 'version', '--registry', stepRegistry],
      { cwd: path.dirname(manifestPath) },
    );
    if (view.status === 0 && String(view.stdout || '').trim().length > 0) {
      versionConflicts.push(`${step.packageName}@${pkg.version}`);
    }
  }
  if (versionConflicts.length > 0) {
    throw new Error(`Publish preflight blocked existing package versions: ${versionConflicts.join(', ')}`);
  }
  return {
    checked: true,
    actor,
    registry,
    tarballsVerified,
    versionConflicts,
  };
}

function createRetryPolicy(options) {
  return {
    maxRetries: options.maxRetries ?? 0,
    retryDelayMs: options.retryDelayMs ?? 1_000,
  };
}

function sleepMs(milliseconds) {
  if (milliseconds <= 0) {
    return;
  }
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
}

function isRetriablePublishFailure(message) {
  return /EAI_AGAIN|ECONNRESET|ECONNREFUSED|ETIMEDOUT|ECONNABORTED|ENOTFOUND|EPIPE|socket hang up|503|502|504|429|rate limit|network/i.test(
    message,
  );
}

function executePublishStep(step, options) {
  const args = [...step.command.args];
  if (options.dryRun && !args.includes('--dry-run')) {
    args.push('--dry-run');
  }
  if (options.otp && !args.includes('--otp')) {
    args.push('--otp', options.otp);
  }
  const resolved = resolveSpawnCommand(options.npmBin ?? step.command.program, args);
  const result = spawnSync(resolved.command, resolved.args, {
    cwd: path.dirname(options.manifestPath),
    encoding: 'utf8',
    env: process.env,
  });
  if (result.status !== 0) {
    throw new Error(
      `${step.command.program} ${args.join(' ')} failed for ${step.packageName}: ${
        result.stderr || result.stdout || 'unknown error'
      }`,
    );
  }
  return args;
}

function executePublishStepWithRetry(manifest, step, options) {
  const maxAttempts = (options.maxRetries ?? 0) + 1;
  let attempt = 0;
  while (attempt < maxAttempts) {
    attempt += 1;
    const startedAt = new Date().toISOString();
    try {
      const args = executePublishStep(step, options);
      manifest.execution.attempts.push({
        stepId: step.id,
        packageName: step.packageName,
        attempt,
        status: 'succeeded',
        retriable: false,
        startedAt,
        finishedAt: new Date().toISOString(),
        args,
        error: null,
      });
      return;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      const retriable = attempt < maxAttempts && isRetriablePublishFailure(message);
      manifest.execution.attempts.push({
        stepId: step.id,
        packageName: step.packageName,
        attempt,
        status: 'failed',
        retriable,
        startedAt,
        finishedAt: new Date().toISOString(),
        args: [...step.command.args],
        error: message,
      });
      if (!retriable) {
        throw error;
      }
      sleepMs(options.retryDelayMs ?? 1_000);
    }
  }
}

function createRollbackPlan(manifest, completedStepIds) {
  if (completedStepIds.length === 0) {
    return {
      status: 'not_needed',
      generatedAt: new Date().toISOString(),
      reason: 'no new packages were published in the failed invocation',
      commands: [],
    };
  }
  const packageByName = new Map((manifest.packages ?? []).map((entry) => [entry.name, entry]));
  const commands = completedStepIds
    .map((stepId) => manifest.steps.find((entry) => entry.id === stepId))
    .filter(Boolean)
    .reverse()
    .map((step) => {
      const pkg = packageByName.get(step.packageName);
      return {
        stepId: step.id,
        packageName: step.packageName,
        version: pkg?.version ?? null,
        registry: step.publishConfig?.registry ?? 'https://registry.npmjs.org/',
        command: [
          'unpublish',
          pkg?.version ? `${step.packageName}@${pkg.version}` : step.packageName,
          '--registry',
          step.publishConfig?.registry ?? 'https://registry.npmjs.org/',
        ],
      };
    });
  return {
    status: 'planned',
    generatedAt: new Date().toISOString(),
    reason: 'publish failed after partial success; review and optionally execute rollback commands in reverse order',
    commands,
  };
}

function executeRollbackPlan(rollbackPlan, options) {
  for (const rollback of rollbackPlan.commands) {
    run(options.npmBin ?? 'npm', rollback.command, {
      cwd: path.dirname(options.manifestPath),
      env: process.env,
    });
  }
  rollbackPlan.status = 'executed';
  rollbackPlan.reason = 'rollback commands executed in reverse publish order';
}

function executeManifest(manifestPath, options) {
  const manifest = readJson(manifestPath);
  if (!Array.isArray(manifest.steps)) {
    throw new Error(`Invalid publish manifest: missing steps in ${manifestPath}`);
  }
  const priorCompleted = options.resume && Array.isArray(manifest.execution?.completedSteps)
    ? manifest.execution.completedSteps.filter((stepId) =>
        manifest.steps.some((step) => step.id === stepId),
      )
    : [];
  manifest.execution = createInitialExecutionState(
    options.dryRun,
    priorCompleted,
    createRetryPolicy(options),
  );
  writeManifest(manifestPath, manifest);
  try {
    manifest.execution.preflight = runPublishPreflight(manifestPath, manifest, options);
    manifest.execution.lastError = null;
    manifest.execution.updatedAt = new Date().toISOString();
    writeManifest(manifestPath, manifest);
  } catch (error) {
    manifest.execution.lastError = error instanceof Error ? error.message : String(error);
    manifest.execution.updatedAt = new Date().toISOString();
    writeManifest(manifestPath, manifest);
    throw error;
  }
  const completedThisRun = [];
  for (const step of manifest.steps) {
    if (manifest.execution.completedSteps.includes(step.id)) {
      continue;
    }
    try {
      executePublishStepWithRetry(manifest, step, {
        dryRun: options.dryRun,
        otp: options.otp,
        npmBin: options.npmBin,
        manifestPath,
        maxRetries: options.maxRetries,
        retryDelayMs: options.retryDelayMs,
      });
      manifest.execution.completedSteps.push(step.id);
      completedThisRun.push(step.id);
      manifest.execution.failedStepId = null;
      manifest.execution.lastError = null;
      manifest.execution.rollbackPlan = createRollbackPlan(manifest, completedThisRun);
      manifest.execution.updatedAt = new Date().toISOString();
      writeManifest(manifestPath, manifest);
    } catch (error) {
      manifest.execution.failedStepId = step.id;
      manifest.execution.lastError = error instanceof Error ? error.message : String(error);
      manifest.execution.rollbackPlan = createRollbackPlan(manifest, completedThisRun);
      if (options.rollbackOnFailure && !options.dryRun && manifest.execution.rollbackPlan.commands.length > 0) {
        try {
          executeRollbackPlan(manifest.execution.rollbackPlan, {
            npmBin: options.npmBin,
            manifestPath,
          });
        } catch (rollbackError) {
          manifest.execution.rollbackPlan.status = 'failed';
          manifest.execution.rollbackPlan.reason =
            rollbackError instanceof Error ? rollbackError.message : String(rollbackError);
        }
      }
      manifest.execution.updatedAt = new Date().toISOString();
      writeManifest(manifestPath, manifest);
      throw error;
    }
  }
  manifest.execution.executed = true;
  manifest.execution.failedStepId = null;
  manifest.execution.lastError = null;
  manifest.execution.rollbackPlan = createRollbackPlan(manifest, []);
  manifest.execution.updatedAt = new Date().toISOString();
  writeManifest(manifestPath, manifest);
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.manifest) {
    executeManifest(path.resolve(repoRoot, args.manifest), {
      dryRun: args.dryRun,
      otp: args.otp,
      npmBin: args.npmBin,
      yes: args.yes,
      resume: args.resume,
      maxRetries: args.maxRetries,
      retryDelayMs: args.retryDelayMs,
      rollbackOnFailure: args.rollbackOnFailure,
    });
    return;
  }

  // Always pack first to ensure:
  // - dist assets exist
  // - @lania-cli/cli contains lib/node-bridge payload
  run('node', ['./scripts/pack.mjs'], { cwd: repoRoot });
  runPnpm(['install', '--no-frozen-lockfile']);

  const publishArgs = ['exec', 'changeset', 'publish'];
  const tag = process.env.NPM_TAG;
  if (tag) {
    publishArgs.push('--tag', tag);
  }
  // Packages are managed as a fixed release group by changesets.
  runPnpm(publishArgs);
}

main();

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');

function parseArgs(argv) {
  const result = {
    manifest: null,
    registry: null,
    output: path.join(repoRoot, '.lania', 'tmp', 'npmrc.publish'),
    scope: null,
    tokenEnv: null,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--manifest') {
      result.manifest = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (arg === '--registry') {
      result.registry = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (arg === '--output') {
      result.output = argv[index + 1] ?? result.output;
      index += 1;
      continue;
    }
    if (arg === '--scope') {
      result.scope = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (arg === '--token-env') {
      result.tokenEnv = argv[index + 1] ?? null;
      index += 1;
    }
  }
  return result;
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function normalizeRegistry(registry) {
  const value = String(registry ?? '').trim();
  if (!value) {
    throw new Error('Registry is required. Pass --registry or --manifest with publish steps.');
  }
  return value.endsWith('/') ? value : `${value}/`;
}

function resolveRegistry(args) {
  if (args.registry) {
    return normalizeRegistry(args.registry);
  }
  if (process.env.LANIA_NPM_REGISTRY) {
    return normalizeRegistry(process.env.LANIA_NPM_REGISTRY);
  }
  if (args.manifest) {
    const manifest = readJson(path.resolve(repoRoot, args.manifest));
    const stepRegistry =
      manifest?.steps?.find?.((entry) => entry?.publishConfig?.registry)?.publishConfig?.registry ??
      readRegistryFromCommandArgs(manifest?.steps?.[0]?.command?.args);
    if (stepRegistry) {
      return normalizeRegistry(stepRegistry);
    }
  }
  if (process.env.npm_config_registry) {
    return normalizeRegistry(process.env.npm_config_registry);
  }
  return 'https://registry.npmjs.org/';
}

function readRegistryFromCommandArgs(args) {
  if (!Array.isArray(args)) {
    return null;
  }
  const registryIndex = args.indexOf('--registry');
  if (registryIndex === -1) {
    return null;
  }
  return args[registryIndex + 1] ?? null;
}

function resolveToken(args) {
  const candidates = [];
  if (args.tokenEnv) {
    candidates.push(args.tokenEnv);
  }
  candidates.push('LANIA_NPM_TOKEN', 'NODE_AUTH_TOKEN', 'NPM_TOKEN');
  for (const envName of candidates) {
    const value = process.env[envName];
    if (typeof value === 'string' && value.trim().length > 0) {
      return { envName, token: value.trim() };
    }
  }
  throw new Error(
    `Missing registry token. Set one of ${candidates
      .map((entry) => `"${entry}"`)
      .join(', ')} or pass --token-env.`,
  );
}

function registryAuthKey(registry) {
  const parsed = new URL(registry);
  const pathname = parsed.pathname.endsWith('/') ? parsed.pathname : `${parsed.pathname}/`;
  return `//${parsed.host}${pathname}:_authToken`;
}

function resolveScope(args) {
  if (args.scope) {
    return args.scope.trim();
  }
  if (process.env.LANIA_NPM_SCOPE) {
    return process.env.LANIA_NPM_SCOPE.trim();
  }
  return null;
}

function ensureParentDir(filePath) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const registry = resolveRegistry(args);
  const { envName, token } = resolveToken(args);
  const scope = resolveScope(args);
  const output = path.resolve(repoRoot, args.output);
  const lines = [
    `registry=${registry}`,
    'always-auth=true',
  ];
  if (scope) {
    lines.push(`${scope}:registry=${registry}`);
  }
  lines.push(`${registryAuthKey(registry)}=${token}`);
  ensureParentDir(output);
  fs.writeFileSync(output, lines.join('\n') + '\n', 'utf8');
  process.stdout.write(
    JSON.stringify(
      {
        ok: true,
        output,
        registry,
        scope,
        tokenEnv: envName,
      },
      null,
      2,
    ) + '\n',
  );
}

main();

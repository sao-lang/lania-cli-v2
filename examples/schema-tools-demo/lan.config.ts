export default {
  extensions: {
    dynamicCommands: true,
  },
  schemaDiscovery: {
    files: ['lania.schemas.ts'],
  },
  tools: {
    allow: ['git', 'pm', 'exec', 'fs', 'log', 'text', 'bridge', 'workspace', 'path', 'result'],
    exec: {
      allowShell: false,
      allowEnvWrite: false,
    },
    fs: {
      writeRoot: '.',
    },
    bridge: {
      allowMethods: ['bridge.*', 'config.*', 'compiler.*', 'lint.*', 'commit*'],
    },
  },
};

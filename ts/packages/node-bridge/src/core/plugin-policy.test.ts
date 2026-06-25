/**
 * 动态插件安全策略的回归测试。
 */
import assert from 'node:assert/strict';
import test from 'node:test';

import {
  createDefaultPluginSecurityPolicy,
  intersectAllowedMethods,
  validatePluginDeclaration,
} from './plugin-policy.js';

test('plugin policy blocks sources outside trust policy', () => {
  const policy = createDefaultPluginSecurityPolicy({
    pluginTrustedSources: ['package'],
  });

  assert.throws(
    () =>
      validatePluginDeclaration(
        '/repo',
        { package: './plugins/demo.cjs' },
        policy,
      ),
    /blocked by trust policy/,
  );
});

test('plugin policy requires signature when enabled', () => {
  const policy = createDefaultPluginSecurityPolicy({
    pluginAllowlist: ['demo-plugin'],
    pluginRequireSignature: true,
  });

  assert.throws(
    () =>
      validatePluginDeclaration(
        '/repo',
        { package: 'demo-plugin', methods: ['demo.run'] },
        policy,
      ),
    /requires signature/,
  );

  const validated = validatePluginDeclaration(
    '/repo',
    {
      package: 'demo-plugin',
      methods: ['demo.run'],
      signature: 'sha256:trusted',
    },
    policy,
  );
  assert.equal(validated.trust, 'allowlist');
});

test('plugin policy intersects declared and allowed methods', () => {
  const methods = intersectAllowedMethods(
    ['demo.run', 'demo.hidden'],
    ['demo.run', 'demo.hidden'],
    createDefaultPluginSecurityPolicy({
      pluginMethodAllowlist: ['demo.run'],
    }),
  );

  assert.deepEqual(methods, ['demo.run']);
});

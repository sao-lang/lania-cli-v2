/**
 * 模板插件的回归测试。
 */
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { templatePlugin } from './template.js';

test('template plugin delegates template listing to templates package', async () => {
  const handled = await templatePlugin.handle('template.list', {});
  const result = handled?.result as
    | {
        templates: string[];
        metadata: Array<{ name: string; schemaVersion: number }>;
      }
    | undefined;

  assert.deepEqual(result?.templates, ['spa-react', 'spa-vue', 'toolkit', 'toolkit-monorepo']);
  assert.ok(
    result?.metadata.some(
      (template) => template.name === 'toolkit' && template.schemaVersion === 1,
    ),
  );
  assert.ok(
    result?.metadata.some(
      (template) => template.name === 'toolkit-monorepo' && template.schemaVersion === 1,
    ),
  );
});

test('template plugin delegates template rendering to templates package', async () => {
  const handled = await templatePlugin.handle('template.render', {
    template: 'toolkit',
    context: {
      projectName: 'demo-kit',
    },
  });
  const result = handled?.result as
    | {
        template: string;
        schemaVersion: number;
        renderEngine: string;
        files: Array<{ path: string; content: string }>;
      }
    | undefined;

  assert.equal(result?.template, 'toolkit');
  assert.equal(result?.schemaVersion, 1);
  assert.equal(result?.renderEngine, 'node_bridge');
  assert.ok(result?.files.some((file) => file.path === 'src/index.ts'));
  assert.ok(result?.files.some((file) => file.content.includes('getToolkitInfo')));
  assert.ok(result?.files.some((file) => file.content.includes('demo-kit')));
});

test('template plugin delegates add template rendering to templates package', async () => {
  const handled = await templatePlugin.handle('addTemplate.render', {
    template: 'rfc',
    context: {
      language: 'ts',
      cssProcessor: 'scss',
      projectName: 'demo-kit',
    },
  });
  const result = handled?.result as
    | {
        template: string;
        filename: string | null;
        extname: string | null;
        content: string;
      }
    | undefined;

  assert.equal(result?.template, 'rfc');
  assert.equal(result?.filename, null);
  assert.equal(result?.extname, 'tsx');
  assert.match(result?.content ?? '', /const MyComponent/);
});

test('template plugin discovers product.templatesDir and merges schema template metadata', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-template-plugin-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  const templateRoot = path.join(cwd, 'templates', 'custom-widget');
  await mkdir(path.join(templateRoot, 'files'), { recursive: true });

  await writeFile(
    path.join(cwd, 'lan.config.js'),
    [
      'export default {',
      '  product: {',
      "    templatesDir: './templates',",
      '  },',
      '};',
      '',
    ].join('\n'),
  );
  await writeFile(
    path.join(cwd, 'lania.schemas.js'),
    [
      'export default {',
      '  templates: [',
      '    {',
      "      id: 'custom-widget',",
      "      title: 'Custom Widget',",
      "      tags: ['internal', 'phase1'],",
      '    },',
      '  ],',
      '};',
      '',
    ].join('\n'),
  );
  await writeFile(
    path.join(templateRoot, 'template.json'),
    JSON.stringify(
      {
        name: 'custom-widget',
        schemaVersion: 1,
        renderEngine: 'node_bridge',
        legacyTemplateDir: 'custom-widget',
        ownership: 'third_party_extension',
        useCases: ['create'],
      },
      null,
      2,
    ),
  );
  await writeFile(
    path.join(templateRoot, 'files', 'package.json.ejs'),
    '{\n  "name": "<%= projectName %>"\n}\n',
  );

  const listed = await templatePlugin.handle('template.list', { cwd });
  const listResult = listed?.result as
    | {
        templates: string[];
        metadata: Array<{ name: string; title?: string; tags?: string[] }>;
      }
    | undefined;

  assert.ok(listResult?.templates.includes('custom-widget'));
  assert.ok(
    listResult?.metadata.some(
      (template) =>
        template.name === 'custom-widget' &&
        template.title === 'Custom Widget' &&
        template.tags?.includes('phase1'),
    ),
  );

  const rendered = await templatePlugin.handle('template.render', {
    cwd,
    template: 'custom-widget',
    context: {
      projectName: 'widget-demo',
    },
  });
  const renderResult = rendered?.result as
    | {
        template: string;
        files: Array<{ path: string; content: string }>;
      }
    | undefined;

  assert.equal(renderResult?.template, 'custom-widget');
  assert.ok(
    renderResult?.files.some(
      (file) => file.path === 'package.json' && file.content.includes('"name": "widget-demo"'),
    ),
  );
});

import test from 'node:test';

import {
  assert,
  createDynamicCommandProject,
  invokeManifestHandler,
  rm,
  writeProjectFile,
} from '../shared.js';

export function registerPresentationServiceTests() {
  test('command.invokeDynamic supports text styling with ANSI rendering', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'text-tools',
                handler: async (ctx) => {
                  const styled = ctx.tools.text
                    .style('ok', {
                      prefix: '[',
                      suffix: ']',
                      color: ctx.tools.text.rgb(12.4, 260, -2),
                      backgroundColor: ctx.tools.text.hsl(0, 100, 50)
                    })
                    .bold()
                    .underline()
                    .overline()
                    .render();
                  const visible = ctx.tools.text.style('peek').hidden().visible().render();
                  const plain = ctx.tools.text.render('plain', { prefix: '<', suffix: '>' });
                  const hsl = ctx.tools.text.hsl(120, 100, 25);
                  return ctx.tools.result.ok({ styled, visible, plain, hsl });
                }
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const invoked = await invokeManifestHandler({
      cwd,
      commandName: 'text-tools',
      requestIdPrefix: 'req-text-tools',
    });

    const payload = (invoked.response.result as any)?.result?.data;
    assert.equal(
      payload?.styled,
      '[\u001b[1m\u001b[4m\u001b[53m\u001b[38;2;12;255;0m\u001b[48;2;255;0;0mok\u001b[0m]',
    );
    assert.equal(payload?.visible, 'peek');
    assert.equal(payload?.plain, '<plain>');
    assert.equal(payload?.hsl?.r, 0);
    assert.equal(payload?.hsl?.g, 128);
    assert.equal(payload?.hsl?.b, 0);
  });

  test('command.invokeDynamic supports expanded config search matrix', async (t) => {
    const cwd = await createDynamicCommandProject({
      configContent: `export default { extensions: { dynamicCommands: true } };`,
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'config-matrix',
                handler: async (ctx) => {
                  const supported = ctx.tools.config.supportedTypes();
                  const markdownlintPlaces = ctx.tools.config.searchPlaces('markdownlint');
                  const gulpPlaces = ctx.tools.config.searchPlaces('gulp');
                  const czPlaces = ctx.tools.config.searchPlaces('cz');
                  const lanPlaces = ctx.tools.config.searchPlaces('lan');
                  const markdownlint = await ctx.tools.config.load('markdownlint');
                  const gulp = await ctx.tools.config.load('gulp');
                  const cz = await ctx.tools.config.loadTool('cz');
                  const lan = await ctx.tools.config.loadLan();
                  const searched = await ctx.tools.config.search('markdownlint');
                  const markdownlintTool = await ctx.tools.config.loadTool('markdownlint');
                  const gulpTool = await ctx.tools.config.loadTool('gulp');
                  return ctx.tools.result.ok({
                    supported,
                    markdownlintPlaces,
                    gulpPlaces,
                    czPlaces,
                    lanPlaces,
                    markdownlint,
                    markdownlintTool,
                    gulp,
                    gulpTool,
                    cz,
                    lan,
                    searched
                  });
                }
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    await writeProjectFile(cwd, '.markdownlint.jsonc', '{"default": true}');
    await writeProjectFile(cwd, 'gulpfile.mjs', 'export default { task: "build" };');
    await writeProjectFile(
      cwd,
      '.czrc.cjs',
      'module.exports = { types: [{ value: "feat", name: "feat" }] };',
    );
    await writeProjectFile(cwd, 'lan.config.mjs', 'export default { app: "lania" };');

    const invoked = await invokeManifestHandler({
      cwd,
      commandName: 'config-matrix',
      requestIdPrefix: 'req-config-matrix',
    });

    assert.equal((invoked.response.result as any)?.result?.error, undefined);
    const payload = (invoked.response.result as any)?.result?.data;
    assert.equal(payload?.supported?.includes('markdownlint'), true);
    assert.equal(payload?.supported?.includes('gulp'), true);
    assert.equal(payload?.supported?.includes('cz'), true);
    assert.equal(payload?.markdownlintPlaces?.includes('.markdownlint.jsonc'), true);
    assert.equal(payload?.gulpPlaces?.includes('gulpfile.mjs'), true);
    assert.equal(payload?.czPlaces?.includes('.czrc.cjs'), true);
    assert.equal(payload?.lanPlaces?.includes('lan.config.mjs'), true);
    assert.equal(payload?.markdownlint?.exists, true);
    assert.equal(payload?.gulp?.exists, true);
    assert.equal(payload?.markdownlintTool?.exists, true);
    assert.equal(payload?.gulpTool?.exists, true);
    assert.equal(payload?.cz?.exists, true);
    assert.equal(payload?.lan?.exists, true);
    assert.equal(payload?.searched?.exists, true);
  });
}

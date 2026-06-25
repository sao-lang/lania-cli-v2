export default {
  runtimeCommands: [
    {
      mount: 'demo',
      command: {
        about: 'Schema tools demo commands',
      },
      commands: [
        {
          name: 'inspect',
          about: 'Inspect the current workspace with ctx.tools',
          hooks: {
            onArgsParsed: [
              async (ctx, payload) => {
                const banner = ctx.tools.text
                  .style('schema-tools demo', {
                    prefix: '[',
                    suffix: ']',
                    color: ctx.tools.text.rgb(64, 160, 255),
                  })
                  .bold()
                  .render();
                const ping = await ctx.tools.bridge.call('bridge.ping');
                return {
                  ...payload,
                  events: [
                    ...(payload?.events ?? []),
                    {
                      method: 'event.log',
                      params: {
                        level: 'info',
                        target: 'example.inline-hook',
                        message: banner,
                      },
                    },
                    {
                      method: 'event.log',
                      params: {
                        level: 'debug',
                        target: 'example.inline-hook',
                        message: `bridge.ping ok=${String((ping.response.result as any)?.ok ?? false)}`,
                      },
                    },
                  ],
                };
              },
            ],
          },
          handler: async (ctx) => {
            const reportPath = ctx.tools.path.resolve(ctx.cwd, '.lania', 'reports', 'schema-tools.json');
            const pkg = await ctx.tools.workspace.packageJson();
            const manager = await ctx.tools.pm.detect();
            const branch = await ctx.tools.git.branch.current();
            const echo = await ctx.tools.exec.spawn('/bin/echo', ['schema-tools-demo'], {
              cwd: ctx.cwd,
            });

            await ctx.tools.fs.ensureDir(ctx.tools.path.dirname(reportPath));
            await ctx.tools.fs.writeJson(reportPath, {
              packageName: pkg?.name ?? null,
              manager,
              branch,
              echo: echo.stdout.trim(),
            });

            await ctx.tools.log.success(`report written: ${reportPath}`, {
              target: 'example.inspect',
            });

            return ctx.tools.result.ok({
              packageName: pkg?.name ?? null,
              manager,
              branch,
              reportPath,
              echo: echo.stdout.trim(),
            });
          },
        },
      ],
    },
  ],
};

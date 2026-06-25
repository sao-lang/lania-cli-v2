export default {
  commands: [
    {
      name: 'hello',
      about: 'Verify the product CLI runtime path',
      workflow: 'hello',
    },
    {
      name: 'deploy',
      about: 'Collect deploy inputs and write a deploy plan',
      options: [
        {
          long: 'service',
          valueKind: 'string',
          help: 'Service name',
          required: true,
        },
        {
          long: 'env',
          valueKind: 'string',
          help: 'Target environment',
          required: true,
        },
        {
          long: 'confirm',
          valueKind: 'bool',
          help: 'Confirm writing the deploy plan',
        },
      ],
      prompt: [
        {
          field: 'service',
          message: 'Service name?',
          kind: 'input',
          whenMissing: ['service'],
        },
        {
          id: 'env',
          field: 'env',
          message: 'Select environment',
          kind: 'select',
          whenMissing: ['env'],
          choices: [
            { label: 'staging', value: 'staging' },
            { label: 'prod', value: 'prod' },
          ],
        },
        {
          field: 'confirm',
          message: 'Write deploy plan?',
          kind: 'confirm',
          defaultValue: true,
          returnable: true,
        },
      ],
      workflow: 'deploy',
    },
    {
      name: 'product-template',
      about: 'Inspect product templates and write a report',
      workflow: 'productTemplate',
    },
  ],
  workflows: {
    async hello(ctx) {
      return {
        result: {
          message: 'hello from Acme CLI',
          binaryName: 'acme',
          workspaceRoot: ctx.runtime.workspaceRoot,
          productRoot: ctx.runtime.productRoot,
          schemaRoot: ctx.runtime.schemaRoot,
          exitCode: 0,
        },
      };
    },
    async deploy(ctx) {
      const service = String(ctx.argv.options.service ?? 'unknown-service');
      const env = String(ctx.argv.options.env ?? 'staging');
      const confirm = Boolean(ctx.argv.options.confirm ?? true);
      const output = '.lania/reports/deploy-plan.json';

      await ctx.tools.fs.ensureDir('.lania/reports');
      await ctx.tools.fs.writeJson(
        output,
        {
          service,
          env,
          confirm,
          binaryName: ctx.product.binaryName,
          workspaceRoot: ctx.runtime.workspaceRoot,
          productRoot: ctx.runtime.productRoot,
        },
        { space: 2 },
      );
      await ctx.tools.log.success(`deploy plan written: ${output}`);

      return {
        result: {
          message: `deploy ${service} to ${env}`,
          service,
          env,
          confirm,
          output,
          exitCode: 0,
        },
      };
    },
    async productTemplate(ctx) {
      const templatesDir = String(ctx.product.templatesDir ?? './product/templates').replace(
        /^\.\//,
        '',
      );
      const manifests = await ctx.tools.fs.glob(`${templatesDir}/**/template.json`);
      const snapshots = [];
      for (const manifestPath of manifests) {
        snapshots.push({
          path: manifestPath,
          manifest: await ctx.tools.fs.readJson(manifestPath),
        });
      }
      await ctx.tools.fs.writeJson(
        '.lania/reports/product-templates.json',
        { templatesDir, manifests: snapshots },
        { space: 2 },
      );
      return {
        result: {
          templateId: 'demo-app',
          templateTitle: 'Demo App',
          templatesCount: Array.isArray(manifests) ? manifests.length : 0,
          output: '.lania/reports/product-templates.json',
          exitCode: 0,
        },
      };
    },
  },
  templates: [
    {
      id: 'demo-app',
      title: 'Demo App',
    },
  ],
};

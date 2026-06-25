/**
 * add 命令使用的轻量模板运行时，渲染单文件或配置片段模板。
 *
 * 主要导出：listAddRuntimeTemplates、getAddRuntimeTemplate、renderAddRuntimeTemplate、AddTemplateManifest。
 *
 * 与 create 模板（template-runtime.ts）不同：
 * - add 模板是“单资产”渲染：给定 name + context，产出一段内容（以及可选 filename/extname）。
 * - 模板资产来自 `add-templates/assets/*.ejs`，运行时优先读取 dist 产物，开发态回退到 src。
 *
 * 典型用途：
 * - 生成组件骨架（rfc/rcc/v2/v3/svelte/astro）
 * - 生成配置片段（eslint/prettier/stylelint/editorconfig/tsconfig/commitizen 等）
 */
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { templatesPackageDir } from './runtime.js';
import { ejsRenderer } from './ejs-renderer.js';

type AddLanguage = 'js' | 'ts';

export interface AddTemplateManifest {
  name: string;
  label: string;
  /**
   * 若指定 filename，则输出文件名固定（例如 prettier.config.js）。
   * 否则由调用方结合 extname 决定最终输出路径。
   */
  filename?: string;
  /**
   * 文件扩展名：
   * - string：固定扩展名（例如 vue / svelte）
   * - Record<AddLanguage, string>：按语言选择（例如 ts -> tsx, js -> jsx）
   */
  extname?: string | Record<AddLanguage, string>;
  schemaVersion: number;
}

interface AddTemplateRuntimeInput {
  context?: Record<string, unknown>;
}

const ADD_TEMPLATES: AddTemplateManifest[] = [
  { name: 'v2', label: 'vue2模板组件', extname: 'vue', schemaVersion: 1 },
  { name: 'v3', label: 'vue3模板组件', extname: 'vue', schemaVersion: 1 },
  {
    name: 'rcc',
    label: 'react类组件',
    extname: { js: 'jsx', ts: 'tsx' },
    schemaVersion: 1,
  },
  {
    name: 'rfc',
    label: 'react函数式组件',
    extname: { js: 'jsx', ts: 'tsx' },
    schemaVersion: 1,
  },
  { name: 'svelte', label: 'svelte模板组件', extname: 'svelte', schemaVersion: 1 },
  { name: 'astro', label: 'astro组件', extname: 'astro', schemaVersion: 1 },
  {
    name: 'prettier',
    label: 'prettier配置文件',
    filename: 'prettier.config.js',
    schemaVersion: 1,
  },
  {
    name: 'eslint',
    label: 'eslint配置文件',
    filename: 'eslint.config.js',
    schemaVersion: 1,
  },
  {
    name: 'stylelint',
    label: 'stylelint配置文件',
    filename: 'stylelint.config.js',
    schemaVersion: 1,
  },
  {
    name: 'editorconfig',
    label: 'editorconfig配置文件',
    filename: '.editorconfig',
    schemaVersion: 1,
  },
  {
    name: 'gitignore',
    label: '.gitignore文件',
    filename: '.gitignore',
    schemaVersion: 1,
  },
  {
    name: 'tsconfig',
    label: 'tsconfig配置文件',
    filename: 'tsconfig.json',
    schemaVersion: 1,
  },
  {
    name: 'commitizen',
    label: 'commitizen配置文件',
    filename: 'cz.config.js',
    schemaVersion: 1,
  },
];

export async function listAddRuntimeTemplates() {
  return ADD_TEMPLATES;
}

export async function getAddRuntimeTemplate(name: string) {
  return ADD_TEMPLATES.find((template) => template.name === name) ?? null;
}

export async function renderAddRuntimeTemplate(
  name: string,
  input: AddTemplateRuntimeInput = {},
) {
  const template = await getAddRuntimeTemplate(name);
  if (!template) {
    throw new Error(`Unknown add template: ${name}`);
  }

  const context = input.context ?? {};
  // add 模板的 language 只区分 js/ts，用于选择 jsx/tsx 等扩展名以及模板内条件逻辑。
  const language = normalizeLanguage(context.language);
  const templateFilePath = resolveAddTemplateAssetPath(name);
  const content = await ejsRenderer.renderFile(templateFilePath, {
    cssProcessor: typeof context.cssProcessor === 'string' ? context.cssProcessor : 'css',
    language,
    name: typeof context.projectName === 'string' ? context.projectName : 'lania-app',
    projectName: typeof context.projectName === 'string' ? context.projectName : 'lania-app',
  });

  return {
    template: template.name,
    schemaVersion: template.schemaVersion,
    label: template.label,
    filename: template.filename ?? null,
    extname: resolveExtname(template, language),
    content,
  };
}

function resolveAddTemplateAssetPath(name: string) {
  const assetName = resolveAssetName(name);
  // 发布态优先读 dist：不依赖 TS 源码与 workspace 目录结构。
  const distPath = resolve(
    templatesPackageDir(),
    'dist',
    'add-templates',
    'assets',
    assetName,
  );
  if (existsSync(distPath)) {
    return distPath;
  }
  // 开发态回退到 src，便于在 monorepo/workspace 中直接调试模板资产。
  return resolve(
    templatesPackageDir(),
    'src',
    'add-templates',
    'assets',
    assetName,
  );
}

function resolveAssetName(name: string) {
  // 某些模板资产文件名以点开头（gitignore/editorconfig），在 npm 包中需要特殊命名以便分发。
  if (name === 'gitignore') {
    return '.ignore.ejs';
  }
  if (name === 'editorconfig') {
    return '.editorconfig.ejs';
  }
  return `${name}.ejs`;
}

function normalizeLanguage(value: unknown): AddLanguage {
  return value === 'js' ? 'js' : 'ts';
}

function resolveExtname(template: AddTemplateManifest, language: AddLanguage) {
  if (!template.extname) {
    return null;
  }
  if (typeof template.extname === 'string') {
    return template.extname;
  }
  return template.extname[language] ?? null;
}

import { defineConfig } from 'vitepress'

// GitHub Pages 部署在 https://<owner>.github.io/<repo>/ 子路径下，构建时通过 DOCS_BASE 指定。
const rawBase = process.env.DOCS_BASE?.trim() || '/'
const base = rawBase === '/' ? '/' : `/${rawBase.replace(/^\/+|\/+$/g, '')}/`

export default defineConfig({
  base,
  lang: 'zh-CN',
  title: 'Lania CLI',
  titleTemplate: ':title · Lania CLI 学习文档',
  description: 'Lania CLI v2 学习文档：双运行时 CLI 宿主、命令、模板、代码生成、发布与产品化。',
  cleanUrls: true,
  lastUpdated: true,
  head: [
    ['link', { rel: 'icon', href: `${base}logo.svg` }],
    ['meta', { name: 'theme-color', content: '#42b883' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:title', content: 'Lania CLI 学习文档' }],
    [
      'meta',
      {
        property: 'og:description',
        content: '从命令到架构，系统学习 Lania CLI v2 的双运行时设计。'
      }
    ]
  ],
  markdown: {
    lineNumbers: true,
    theme: {
      light: 'github-light',
      dark: 'github-dark'
    }
  },
  themeConfig: {
    logo: '/logo.svg',
    siteTitle: 'Lania CLI',
    nav: [
      { text: '指南', link: '/guide/introduction', activeMatch: '/guide/' },
      { text: '命令', link: '/reference/cli', activeMatch: '/reference/' },
      { text: '架构', link: '/architecture/overview', activeMatch: '/architecture/' },
      { text: '实战', link: '/cookbook/create-project', activeMatch: '/cookbook/' },
      { text: '贡献', link: '/contributing/development', activeMatch: '/contributing/' }
    ],
    sidebar: {
      '/guide/': [
        {
          text: '开始',
          items: [
            { text: '认识 Lania CLI', link: '/guide/introduction' },
            { text: '命令模型与加载原理', link: '/guide/command-model' },
            { text: '快速上手', link: '/guide/getting-started' },
            { text: '安装与运行原理', link: '/guide/installation' },
            { text: '创建第一个项目', link: '/guide/first-project' }
          ]
        },
        {
          text: '核心功能',
          items: [
            { text: '命令总览', link: '/guide/commands' },
            { text: '工具命令 tools', link: '/guide/tools' },
            { text: '模板与脚手架', link: '/guide/scaffolding' },
            { text: '代码生成', link: '/guide/generate' },
            { text: '发布与同步', link: '/guide/release' },
            { text: '产品 CLI', link: '/guide/product-cli' }
          ]
        },
        {
          text: '进阶',
          items: [
            { text: '配置文件', link: '/guide/configuration' },
            { text: '动态命令与插件', link: '/guide/dynamic-commands' },
            { text: '开发运行时命令', link: '/guide/runtime-commands' },
            { text: 'ctx.tools 宿主能力', link: '/guide/host-tools' },
            { text: '输出、语言与进度', link: '/guide/output-and-locale' },
            { text: '常见问题', link: '/guide/troubleshooting' }
          ]
        }
      ],
      '/reference/': [
        {
          text: '参考',
          items: [
            { text: 'CLI 命令参考', link: '/reference/cli' },
            { text: '配置字段参考', link: '/reference/config' },
            { text: '运行时清单参考', link: '/reference/manifest' },
            { text: 'Bridge 方法目录', link: '/reference/bridge-methods' },
            { text: 'Host RPC 方法目录', link: '/reference/host-rpc' },
            { text: 'ctx.tools API', link: '/reference/host-tools' },
            { text: '环境变量', link: '/reference/environment' },
            { text: '输出与状态文件', link: '/reference/output-and-state' },
            { text: '模板参考', link: '/reference/templates' },
            { text: 'Schema 与协议', link: '/reference/schemas' }
          ]
        }
      ],
      '/architecture/': [
        {
          text: '架构',
          items: [
            { text: '整体架构', link: '/architecture/overview' },
            { text: 'Rust 宿主运行时', link: '/architecture/rust-host' },
            { text: 'Node Bridge', link: '/architecture/node-bridge' },
            { text: 'Rust ↔ Node 通信协议', link: '/architecture/bridge-protocol' },
            { text: '插件与 Hook 系统', link: '/architecture/plugin-system' }
          ]
        }
      ],
      '/cookbook/': [
        {
          text: '实战',
          items: [
            { text: '创建项目', link: '/cookbook/create-project' },
            { text: '生成 HTTP / gRPC 代码', link: '/cookbook/generate-api' },
            { text: '生成 Go 模块', link: '/cookbook/generate-module' },
            { text: '打包并发布产品 CLI', link: '/cookbook/product-publish' }
          ]
        }
      ],
      '/contributing/': [
        {
          text: '参与贡献',
          items: [
            { text: '本地开发', link: '/contributing/development' },
            { text: '测试与验收', link: '/contributing/testing' },
            { text: '文档站维护', link: '/contributing/docs' }
          ]
        }
      ]
    },
    search: {
      provider: 'local',
      options: {
        translations: {
          button: {
            buttonText: '搜索文档',
            buttonAriaLabel: '搜索文档'
          },
          modal: {
            noResultsText: '没有找到相关内容',
            resetButtonTitle: '清除查询条件',
            footer: {
              selectText: '选择',
              navigateText: '切换',
              closeText: '关闭'
            }
          }
        }
      }
    },
    outline: {
      level: [2, 3],
      label: '本页大纲'
    },
    docFooter: {
      prev: '上一篇',
      next: '下一篇'
    },
    lastUpdated: {
      text: '最后更新于',
      formatOptions: {
        dateStyle: 'short',
        timeStyle: 'short'
      }
    },
    darkModeSwitchLabel: '外观',
    lightModeSwitchTitle: '切换到浅色模式',
    darkModeSwitchTitle: '切换到深色模式',
    sidebarMenuLabel: '菜单',
    returnToTopLabel: '回到顶部',
    langMenuLabel: '切换语言',
    footer: {
      message: '基于 MIT License 发布 · 文档由 VitePress 构建',
      copyright: 'Copyright © Lania CLI Team'
    }
  }
})

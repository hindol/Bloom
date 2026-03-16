// @ts-check
/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Bloom',
  tagline: 'A local-first, Vim-modal note-taking app',
  favicon: 'img/favicon.ico',
  url: 'https://hindol.github.io',
  baseUrl: '/Bloom/',
  organizationName: 'hindol',
  projectName: 'Bloom',
  onBrokenLinks: 'warn',
  onBrokenMarkdownLinks: 'warn',
  markdown: { hooks: { onBrokenMarkdownImages: 'warn' } },
  i18n: { defaultLocale: 'en', locales: ['en'] },

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          sidebarPath: './sidebars.js',
          routeBasePath: '/',
        },
        blog: false,
        theme: { customCss: './src/css/custom.css' },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      navbar: {
        title: 'Bloom',
        items: [
          { type: 'docSidebar', sidebarId: 'docs', position: 'left', label: 'Docs' },
          { href: 'https://github.com/hindol/Bloom', label: 'GitHub', position: 'right' },
        ],
      },
      footer: {
        style: 'dark',
        copyright: `Bloom — local-first notes with Vim soul.`,
      },
      colorMode: { defaultMode: 'dark', respectPrefersColorScheme: true },
    }),
};

module.exports = config;

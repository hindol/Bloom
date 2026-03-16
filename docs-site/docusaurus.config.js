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
        title: 'Bloom 🌱',
        items: [
          { type: 'docSidebar', sidebarId: 'docs', position: 'left', label: 'Docs' },
          { href: '/Bloom/api/bloom_core', label: 'API', position: 'left' },
          { href: 'https://github.com/hindol/Bloom', label: 'GitHub', position: 'right' },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Docs',
            items: [
              { label: 'Home', to: '/' },
              { label: 'Editing', to: '/features/editing' },
              { label: 'Search', to: '/features/search' },
              { label: 'Block History', to: '/features/block-history' },
            ],
          },
          {
            title: 'Developer',
            items: [
              { label: 'API Docs (cargo doc)', href: '/Bloom/api/bloom_core' },
              { label: 'GitHub', href: 'https://github.com/hindol/Bloom' },
              { label: 'Releases', href: 'https://github.com/hindol/Bloom/releases' },
            ],
          },
          {
            title: 'More',
            items: [
              { label: 'Architecture', href: 'https://github.com/hindol/Bloom/blob/main/docs/ARCHITECTURE.md' },
              { label: 'Design Goals', href: 'https://github.com/hindol/Bloom/blob/main/docs/GOALS.md' },
            ],
          },
        ],
        copyright: `Bloom — local-first notes with Vim soul.`,
      },
      colorMode: { defaultMode: 'dark', respectPrefersColorScheme: true },
    }),
};

module.exports = config;

/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  docs: [
    {
      type: 'doc',
      id: 'getting-started',
      label: 'Home',
    },
    {
      type: 'category',
      label: 'Features',
      items: [
        'features/editing',
        'features/search',
        'features/block-history',
      ],
    },
    {
      type: 'link',
      label: 'API Docs (cargo doc)',
      href: '/Bloom/api/bloom_core',
    },
  ],
};

module.exports = sidebars;

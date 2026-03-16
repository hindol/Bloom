/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  docs: [
    'getting-started',
    {
      type: 'category',
      label: 'Features',
      items: [
        'features/editing',
        'features/search',
        'features/block-history',
      ],
    },
  ],
};

module.exports = sidebars;

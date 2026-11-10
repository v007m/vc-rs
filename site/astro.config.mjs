// @ts-check
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import svelte from '@astrojs/svelte';

/** The Pages path; llms.txt, the schema URLs and every link derive from it and `site`. */
const base = '/vc-rs';

const dir = (label, directory, collapsed = false) => ({ label, collapsed, items: [{ autogenerate: { directory } }] });

export default defineConfig({
  site: 'https://v007m.github.io',
  base,
  trailingSlash: 'always',
  integrations: [
    svelte(),
    starlight({
      title: 'Vibranium',
      description:
        'Vibranium (vc) is one fast engine for functional, contract and load tests of HTTP APIs, written as plain YAML in git.',
      logo: { src: './src/assets/logo.svg', replacesTitle: false },
      favicon: '/favicon.svg',
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/v007m/vc-rs' }],
      editLink: { baseUrl: 'https://github.com/v007m/vc-rs/edit/master/site/' },
      lastUpdated: true,
      customCss: [
        '@fontsource/jetbrains-mono/400.css',
        '@fontsource/jetbrains-mono/600.css',
        './src/styles/custom.css',
      ],
      head: [
        { tag: 'link', attrs: { rel: 'alternate', type: 'text/plain', href: `${base}/llms.txt`, title: 'llms.txt' } },
      ],
      sidebar: [
        dir('Start here', 'start'),
        { label: 'Playground', link: '/playground/' },
        dir('Walkthroughs', 'walkthroughs'),
        {
          label: 'Syntax reference',
          items: [
            { label: 'Keyword index', link: '/reference/' },
            { label: 'JSON Schemas', link: '/reference/schemas/' },
            { autogenerate: { directory: 'reference/syntax' } },
          ],
        },
        {
          label: 'Config & templates',
          items: [
            dir('Config', 'reference/config'),
            dir('Templates', 'reference/templates'),
            dir('Pipes', 'reference/pipes'),
            dir('Matchers', 'reference/matchers'),
          ],
        },
        {
          label: 'Lifecycle & data',
          items: [dir('Lifecycle', 'reference/lifecycle'), dir('Data', 'reference/data'), dir('Auth', 'reference/auth')],
        },
        dir('Load testing', 'reference/load'),
        dir('Plugins & alerts', 'reference/plugins'),
        dir('CLI', 'reference/cli'),
        dir('Guides', 'guides'),
        dir('For AI agents', 'agents'),
        dir('Diagnostics', 'reference/diagnostics', true),
      ],
    }),
  ],
  vite: {
    resolve: {
      alias: { '~': fileURLToPath(new URL('./src', import.meta.url)) },
    },
  },
});

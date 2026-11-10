import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [
    sveltekit({
      // A single-page app: `vc ui` embeds `build/` and serves index.html for every unknown path.
      adapter: adapter({ fallback: 'index.html', strict: true }),
    }),
  ],
  // `pnpm dev` proxies the engine API to a `vc ui --no-open` on its default port, so the
  // session cookie stays same-origin as it is in the embedded build.
  server: {
    proxy: { '/api': 'http://127.0.0.1:7420' },
  },
});

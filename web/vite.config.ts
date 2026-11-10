import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const vcUi = 'http://127.0.0.1:7420';

export default defineConfig({
  plugins: [
    sveltekit({
      // A single-page app: `vc ui` embeds `build/` and serves index.html for every unknown path.
      adapter: adapter({ fallback: 'index.html', strict: true }),
      // `vc ui` sends `default-src 'self'`; the one inline bootstrap script is allowed by its
      // hash, which the build writes into index.html's CSP meta tag.
      csp: { mode: 'hash', directives: { 'default-src': ['self'], 'script-src': ['self'] } },
    }),
  ],
  // `pnpm dev` proxies the engine API to a running `vc ui --no-open`. The dev page's origin is
  // not vc ui's, so the proxy presents vc ui's origin and the token pasted from the URL it
  // printed (VC_UI_TOKEN); vc itself never reads that variable.
  server: {
    proxy: {
      '/api': {
        target: vcUi,
        changeOrigin: true,
        headers: {
          origin: vcUi,
          ...(process.env.VC_UI_TOKEN && { authorization: `Bearer ${process.env.VC_UI_TOKEN}` }),
        },
      },
    },
  },
});

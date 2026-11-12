import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, type Logger, type Plugin } from 'vite';
import { announcerStyle, styleHash } from './src/lib/dev/csp.ts';
import { fromDevPage } from './src/lib/dev/origin.ts';

const vcUi = 'http://127.0.0.1:7420';

/**
 * Opens vc ui's launch link with `token`, as a browser does, and returns the session cookie it
 * sets (`vc_token_<port>=…`), or `undefined` when vc ui refuses the token. Neither is logged.
 */
async function exchange(token: string): Promise<string | undefined> {
  const launch = new URL('/', vcUi);
  launch.searchParams.set('token', token);
  const response = await fetch(launch, { redirect: 'manual' });
  return response.headers
    .getSetCookie()
    .map((line) => line.split(';', 1)[0] ?? '')
    .find((pair) => pair.startsWith(`vc_token_${launch.port}=`));
}

/** The session cookie for `token`, warning once when vc ui refuses it or cannot be reached. */
function signIn(token: string, logger: Logger, retry: () => void): Promise<string | undefined> {
  return exchange(token).then(
    (cookie) => {
      if (!cookie)
        logger.warn(
          'vc ui refused VC_UI_TOKEN: its launch link works once. Restart vc ui and set the new token.',
        );
      return cookie;
    },
    () => {
      logger.warn(`Could not reach vc ui at ${vcUi}; start vc ui --no-open first.`);
      retry();
      return undefined;
    },
  );
}

/**
 * `pnpm dev` signs in to a running `vc ui --no-open` with the one-time launch token printed in
 * its URL (VC_UI_TOKEN; vc itself never reads that variable): the first /api request makes the
 * exchange, and every proxied /api request then carries the session cookie, held only in memory.
 * The proxy answers only the page this dev server served (`fromDevPage`); anything else is 403
 * and never reaches vc ui, since the proxy would present vc ui's origin and the cookie for it.
 */
function vcSession(): Plugin {
  let session: Promise<string | undefined> | undefined;
  return {
    name: 'vc-ui-session',
    apply: 'serve',
    configureServer({ config, middlewares }) {
      const token = process.env.VC_UI_TOKEN;
      middlewares.use((req, res, next) => {
        if (!req.url?.startsWith('/api/')) return next();
        if (!fromDevPage(req.headers)) {
          res.statusCode = 403;
          res.end('Only the page this dev server serves may call /api.');
          return;
        }
        if (!token) return next();
        session ??= signIn(token, config.logger, () => (session = undefined));
        void session.then((cookie) => {
          if (cookie) req.headers.cookie = cookie;
          next();
        });
      });
    },
  };
}

export default defineConfig({
  plugins: [
    sveltekit({
      // A single-page app: `vc ui` embeds `build/` and serves index.html for every unknown path.
      adapter: adapter({ fallback: 'index.html', strict: true }),
      // `vc ui` sends `default-src 'self'`; the one inline bootstrap script is allowed by its
      // hash, which the build writes into index.html's CSP meta tag. So is the one inline style,
      // SvelteKit's route announcer, by its hash only (D311).
      csp: {
        mode: 'hash',
        directives: {
          'default-src': ['self'],
          'script-src': ['self'],
          'style-src': ['self', 'unsafe-hashes', styleHash(announcerStyle)],
        },
      },
    }),
    vcSession(),
  ],
  // `pnpm dev` proxies the engine API to a running `vc ui --no-open`. The dev page's origin is
  // not vc ui's, so the proxy presents vc ui's origin, and `vcSession` adds the session cookie.
  // No CORS: Vite would otherwise let any localhost page read what the proxy answers.
  server: {
    cors: false,
    proxy: {
      '/api': { target: vcUi, changeOrigin: true, headers: { origin: vcUi } },
    },
  },
});

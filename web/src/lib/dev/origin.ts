/** The request headers the dev-server guard reads; Node's `IncomingHttpHeaders` fits. */
export type RequestHeaders = Readonly<Record<string, string | string[] | undefined>>;

/** Host names that reach only this machine, as `URL.hostname` spells them. */
const loopback = new Set(['localhost', '127.0.0.1', '[::1]']);

function single(value: string | string[] | undefined): string | undefined {
  return Array.isArray(value) ? undefined : value;
}

/** The dev server's own origin, from a `Host` that names a loopback address, else `undefined`. */
function ownOrigin(host: string): string | undefined {
  try {
    const url = new URL(`http://${host}`);
    return url.host === host && loopback.has(url.hostname) ? url.origin : undefined;
  } catch {
    return undefined;
  }
}

/**
 * Whether a request to `pnpm dev` comes from a page that dev server served: `Host` is a loopback
 * name (not a LAN address under `vite --host`, nor a rebound DNS name), and the browser says the
 * request is same-origin, by `Origin` equal to that host's origin, by `Sec-Fetch-Site:
 * same-origin`, or both. A page on another port fails, since cookies ignore ports but origins
 * do not; a request with neither header is not from a browser page and fails too.
 */
export function fromDevPage(headers: RequestHeaders): boolean {
  const host = single(headers.host);
  const origin = single(headers.origin);
  const site = single(headers['sec-fetch-site']);
  const own = host ? ownOrigin(host) : undefined;
  if (!own || (origin === undefined && site === undefined)) return false;
  return (origin === undefined || origin === own) && (site === undefined || site === 'same-origin');
}

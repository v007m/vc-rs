/** A piece of a request URL: plain text, or a `${var}` the engine fills in at send time. */
export type UrlPart = Readonly<{ text: string; name?: string }>;

const token = /^\$\{([^}]+)\}$/;

/** `url` split into text and `${var}` tokens, in order. */
export function urlParts(url: string): UrlPart[] {
  return url
    .split(/(\$\{[^}]+\})/)
    .filter((text) => text !== '')
    .map((text) => {
      const name = token.exec(text)?.[1];
      return name === undefined ? { text } : { text, name: name.trim() };
    });
}

/** A cookie the response set, from one `set-cookie` header. */
export type Cookie = Readonly<{
  name: string;
  value: string;
  domain: string;
  path: string;
  expires: string;
  flags: string;
}>;

function split(pair: string): [string, string] {
  const at = pair.indexOf('=');
  return at < 0 ? [pair.trim(), ''] : [pair.slice(0, at).trim(), pair.slice(at + 1).trim()];
}

function expiry(attrs: ReadonlyMap<string, string>): string {
  const age = attrs.get('max-age');
  return attrs.get('expires') ?? (age === undefined ? 'Session' : `in ${age} s`);
}

function flags(attrs: ReadonlyMap<string, string>): string {
  const site = attrs.get('samesite');
  return [
    ...(attrs.has('httponly') ? ['HttpOnly'] : []),
    ...(attrs.has('secure') ? ['Secure'] : []),
    ...(site ? [`SameSite=${site}`] : []),
  ].join(' · ');
}

function cookie(line: string): Cookie {
  const [pair = '', ...rest] = line.split(';');
  const [name, value] = split(pair);
  const attrs = new Map(rest.map(split).map(([k, v]): [string, string] => [k.toLowerCase(), v]));
  const at = (key: string) => attrs.get(key) ?? '';
  return {
    name,
    value,
    domain: at('domain'),
    path: at('path'),
    expires: expiry(attrs),
    flags: flags(attrs),
  };
}

/** The cookies that the response's `set-cookie` headers set. */
export function cookies(headers: readonly (readonly [string, string])[]): Cookie[] {
  return headers
    .filter(([name]) => name.toLowerCase() === 'set-cookie')
    .flatMap(([, value]) => value.split('\n'))
    .map(cookie)
    .filter((c) => c.name !== '');
}

/** A byte count as people read it: `512 B`, `1.8 KB`, `2.4 MB`. */
export function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** A status code's class, `2` for any 2xx, for the status chip's colour (shown with its code). */
export function statusClass(status: number): string {
  return String(Math.floor(status / 100));
}

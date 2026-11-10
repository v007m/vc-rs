/** A response as the api's checks saw it, with secrets already masked by the engine. */
export type CallResponse = Readonly<{
  status: number;
  headers: readonly (readonly [name: string, value: string])[];
  body: unknown;
}>;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function headerPairs(headers: unknown): [string, string][] {
  if (!isRecord(headers)) return [];
  return Object.entries(headers).map(([name, value]) => [
    name,
    Array.isArray(value) ? value.join(', ') : String(value),
  ]);
}

/** Reads the engine's `response` value (`status`, `headers`, `body`); `undefined` when absent or malformed. */
export function parseResponse(json: string | undefined): CallResponse | undefined {
  if (!json) return undefined;
  let value: unknown;
  try {
    value = JSON.parse(json);
  } catch {
    return undefined;
  }
  if (!isRecord(value) || typeof value.status !== 'number') return undefined;
  return { status: value.status, headers: headerPairs(value.headers), body: value.body };
}

/** The first line of a call's text: `200 OK · 148 ms · api expect passed`. */
export function headline(text: string): string {
  return text.split('\n', 1)[0] ?? '';
}

/**
 * The paths of every text, number, boolean or null in `body`, as `--expect-field` takes them
 * (`body.user.email`, `body.items[0].id`). Lists and objects are not offered (D176), nor keys
 * with a `.` or `[` in them, which a dotted path cannot name.
 */
export function scalarFields(body: unknown, prefix = 'body'): string[] {
  if (Array.isArray(body)) return body.flatMap((item, i) => scalarFields(item, `${prefix}[${i}]`));
  if (isRecord(body))
    return Object.entries(body)
      .filter(([name]) => !/[.[\]]/.test(name))
      .flatMap(([name, value]) => scalarFields(value, `${prefix}.${name}`));
  return prefix === 'body' ? [] : [prefix];
}

/** A var as `vc call --var` takes it. */
export function varArg(name: string, value: string): string {
  return `${name}=${value}`;
}

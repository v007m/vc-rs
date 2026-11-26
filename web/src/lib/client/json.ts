/** One line of a JSON tree: an object or list opening, its closing, or a value. */
export type JsonRow = Readonly<{
  /** The value's JSONPath from the body: `$`, `$.orders[0].id`. */
  path: string;
  depth: number;
  key?: string | number;
  kind: 'open' | 'close' | 'leaf';
  /** `{` or `[` for an opening and its closing. */
  bracket?: '{' | '[';
  value?: unknown;
  /** How many keys or items an object or list holds. */
  size?: number;
  collapsed?: boolean;
}>;

/** What a body filter matched: the paths themselves and every path above or under one. */
export type FilterHits = Readonly<{ exact: ReadonlySet<string>; hits: ReadonlySet<string> }>;

export type BodyFilter = FilterHits | Readonly<{ cel: true }> | undefined;

const identifier = /^[A-Za-z_$][\w$]*$/;

/** The path of `key` under `path`: `.name`, `[0]`, or `["odd key"]`. */
export function childPath(path: string, key: string | number): string {
  if (typeof key === 'number') return `${path}[${key}]`;
  return identifier.test(key) ? `${path}.${key}` : `${path}[${JSON.stringify(key)}]`;
}

function entries(value: object): [string | number, unknown][] {
  if (Array.isArray(value)) return value.map((item: unknown, i): [number, unknown] => [i, item]);
  return Object.entries(value);
}

type Walk = Readonly<{ collapsed: ReadonlySet<string>; rows: JsonRow[] }>;

function walk(walker: Walk, at: Omit<JsonRow, 'kind'>): void {
  const { value, path, depth, key } = at;
  const named = key === undefined ? {} : { key };
  if (value === null || typeof value !== 'object') {
    walker.rows.push({ path, depth, ...named, kind: 'leaf', value });
    return;
  }
  const bracket = Array.isArray(value) ? '[' : '{';
  const items = entries(value);
  const collapsed = walker.collapsed.has(path);
  walker.rows.push({ path, depth, ...named, kind: 'open', bracket, size: items.length, collapsed });
  if (collapsed) return;
  for (const [k, v] of items) {
    walk(walker, { path: childPath(path, k), depth: depth + 1, key: k, value: v });
  }
  walker.rows.push({ path, depth, kind: 'close', bracket });
}

/** `body` as tree lines, skipping what sits under a collapsed path. */
export function jsonRows(body: unknown, collapsed: ReadonlySet<string> = new Set()): JsonRow[] {
  const walker: Walk = { collapsed, rows: [] };
  walk(walker, { path: '$', depth: 0, value: body });
  return walker.rows;
}

const special = new Set(['.', '+', '?', '^', '$', '{', '}', '(', ')', '|', '\\', '[', ']']);

function escape(text: string): string {
  return [...text].map((c) => (special.has(c) ? `\\${c}` : c)).join('');
}

/** `query` as a pattern over paths: `[*]` is any index and `.*` any key. */
function pattern(query: string): RegExp {
  const bare = query.replace(/^(\$|body)/, '');
  const rooted = bare === '' || bare.startsWith('.') || bare.startsWith('[') ? bare : `.${bare}`;
  const parts = rooted.split(/(\[\*\]|\.\*)/).map((part) => {
    if (part === '[*]') return '\\[\\d+\\]';
    return part === '.*' ? '\\.[^.[]+' : escape(part);
  });
  return new RegExp(`^\\$${parts.join('')}$`);
}

/**
 * The paths of `rows` that `query` names: a JSONPath such as `$.orders[*].id` or `body.id`.
 * An expression with operators or calls is CEL, which needs the engine to evaluate.
 */
export function filterBody(rows: readonly JsonRow[], query: string): BodyFilter {
  const q = query.trim();
  if (q === '') return undefined;
  if (/[()=<>!&|]/.test(q)) return { cel: true };
  const re = pattern(q);
  const paths = rows.filter((r) => r.kind !== 'close').map((r) => r.path);
  const exact = new Set(paths.filter((p) => re.test(p)));
  const near = (p: string, x: string) => p === x || x.startsWith(p) || p.startsWith(x);
  const hits = new Set(paths.filter((p) => [...exact].some((x) => near(p, x))));
  return { exact, hits };
}

/** A value's JSON type as a check names it. */
export function jsonType(value: unknown): string {
  if (value === null) return 'null';
  if (Array.isArray(value)) return 'list';
  return typeof value === 'boolean' ? 'bool' : typeof value;
}

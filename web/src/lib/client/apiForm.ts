import { token, type PointerEdit } from '#lib/settings/configEdits.ts';
import { MASKED } from '#lib/settings/port.ts';

/** The methods an api can name (syntax reference, Api files). */
export const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'] as const;

/** The timing phases a check can read. */
export const PHASES = ['total', 'dns', 'connect', 'tls', 'firstByte', 'download'] as const;

/** A query parameter or header; one that is off is left out of the call and the file. */
export type Row = { name: string; value: string; on: boolean };

export type BodyMode = 'none' | 'json' | 'text';

/**
 * What the Client's form edits of an api, as its file writes it: templates as written, and a
 * value that reads a secret masked. A masked value is shown, never written back.
 */
export type ApiForm = {
  method: string;
  path: string;
  query: Row[];
  headers: Row[];
  bodyMode: BodyMode;
  body: string;
};

export type CheckKind = 'status' | 'body' | 'timing';
export type CheckOp = 'equals' | 'contains' | 'exists' | 'lt' | 'gt';

/** A check added for this request alone: sent with the call, never saved to the api. */
export type AddedCheck = Readonly<{ kind: CheckKind; path: string; op: CheckOp; value: string }>;

/** An api as `ExplainApi` gives it: its YAML as JSON. */
export type WrittenApi = Readonly<Record<string, unknown>>;

const isMap = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const text = (value: unknown): string =>
  typeof value === 'string' ? value : JSON.stringify(value);

function rows(map: unknown): Row[] {
  if (!isMap(map)) return [];
  return Object.entries(map).map(([name, value]) => ({ name, value: text(value), on: true }));
}

function bodyOf(body: unknown): Pick<ApiForm, 'bodyMode' | 'body'> {
  if (body === undefined) return { bodyMode: 'none', body: '' };
  if (typeof body === 'string') return { bodyMode: 'text', body };
  return { bodyMode: 'json', body: JSON.stringify(body, null, 2) };
}

/** The form for the api `api`. */
export function formOf(api: WrittenApi): ApiForm {
  const { method, path } = api;
  return {
    method: typeof method === 'string' ? method : 'GET',
    path: typeof path === 'string' ? path : '',
    query: rows(api['query']),
    headers: rows(api['headers']),
    ...bodyOf(api['body']),
  };
}

/** Why the form's body cannot be sent, if it cannot. */
export function bodyProblem(form: ApiForm): string | undefined {
  if (form.bodyMode !== 'json') return undefined;
  try {
    JSON.parse(form.body);
    return undefined;
  } catch {
    return 'The body is not JSON. Fix it, or send it as text.';
  }
}

const writable = (value: string): boolean => !value.includes(MASKED);

function live(list: readonly Row[]): Map<string, string> {
  const on = list.filter((row) => row.on && row.name.trim() !== '');
  return new Map(on.map((row) => [row.name.trim(), row.value]));
}

/** The rows of `after` that hold the mask under a name `before` does not have: renamed. */
function renamed(before: Map<string, string>, after: Map<string, string>): [string, string][] {
  return [...after].filter(([name, value]) => !before.has(name) && !writable(value));
}

/**
 * The edits that turn the map at `at` from `base`'s rows into `form`'s. A masked row that was
 * renamed keeps its old name, since its value cannot be written under the new one.
 */
function mapEdits(at: string, base: readonly Row[], form: readonly Row[]): PointerEdit[] {
  const [before, after] = [live(base), live(form)];
  if (after.size === 0) return before.size === 0 ? [] : [{ pointer: at, op: 'remove' }];
  const moved = new Set(renamed(before, after).map(([, value]) => value));
  const removed = [...before]
    .filter(([name, value]) => !after.has(name) && !moved.has(value))
    .map(([name]) => name);
  const changed = [...after].filter(
    ([name, value]) => before.get(name) !== value && writable(value),
  );
  return [
    ...removed.map((name): PointerEdit => ({ pointer: `${at}/${token(name)}`, op: 'remove' })),
    ...changed.map(([name, value]): PointerEdit => ({
      pointer: `${at}/${token(name)}`,
      op: 'set',
      value,
    })),
  ];
}

function bodyEdits(at: string, base: ApiForm, form: ApiForm): PointerEdit[] {
  const same = base.bodyMode === form.bodyMode && base.body === form.body;
  if (same || !writable(form.body) || bodyProblem(form) !== undefined) return [];
  const pointer = `${at}/body`;
  if (base.bodyMode === 'text' && form.bodyMode === 'text') {
    return [{ pointer, op: 'set', value: form.body }];
  }
  const cleared: PointerEdit[] = base.bodyMode === 'none' ? [] : [{ pointer, op: 'remove' }];
  if (form.bodyMode === 'none') return cleared;
  const value: unknown = form.bodyMode === 'json' ? JSON.parse(form.body) : form.body;
  return [...cleared, { pointer, op: 'set', value }];
}

/**
 * The edits that turn the api at `at` (`/apis/1`) as `base` read it into `form`; none when they
 * agree. These are what a save writes and what a call is sent with.
 */
export function editsBetween(at: string, base: ApiForm, form: ApiForm): PointerEdit[] {
  const scalars = (['method', 'path'] as const).filter(
    (key) => base[key] !== form[key] && writable(form[key]),
  );
  return [
    ...scalars.map((key): PointerEdit => ({
      pointer: `${at}/${key}`,
      op: 'set',
      value: form[key],
    })),
    ...mapEdits(`${at}/query`, base.query, form.query),
    ...mapEdits(`${at}/headers`, base.headers, form.headers),
    ...bodyEdits(at, base, form),
  ];
}

const kept = (what: string): string =>
  `${what} holds a secret, so the form does not save it; edit it in the api file.`;

/** Why each changed row of a map that holds the mask is not saved. */
function rowNotices(noun: string, base: readonly Row[], form: readonly Row[]): string[] {
  const [before, after] = [live(base), live(form)];
  const moved = new Set(renamed(before, after).map(([name]) => name));
  return [...after]
    .filter(([name, value]) => before.get(name) !== value && !writable(value))
    .map(([name]) =>
      moved.has(name)
        ? `${noun} ${name} holds a secret, so it is not saved under a new name; rename it in the api file.`
        : kept(`${noun} ${name}`),
    );
}

/** Why each edit `form` makes to a value that holds the mask is not saved; none when none is. */
export function maskedNotices(base: ApiForm, form: ApiForm): string[] {
  const scalars = (['method', 'path'] as const).filter(
    (key) => base[key] !== form[key] && !writable(form[key]),
  );
  const body = base.bodyMode !== form.bodyMode || base.body !== form.body;
  return [
    ...scalars.map((key) => kept(`The ${key}`)),
    ...rowNotices('Query parameter', base.query, form.query),
    ...rowNotices('Header', base.headers, form.headers),
    ...(body && !writable(form.body) ? [kept('The body')] : []),
  ];
}

/** `text` as the JSON it reads as, else the text: `201` is a number, `500ms` text. */
function literal(value: string): unknown {
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

/** Where a check sits under `expect`. */
function place(check: AddedCheck): string[] {
  const path = check.path.trim();
  if (check.kind === 'status') return ['status'];
  return check.kind === 'body' ? ['body', 'match', path] : ['timing', path];
}

function expected(check: AddedCheck): unknown {
  if (check.op === 'equals') return literal(check.value.trim());
  if (check.op === 'exists') return { exists: check.value.trim() !== 'false' };
  return { [check.op]: literal(check.value.trim()) };
}

function has(api: WrittenApi, keys: readonly string[]): boolean {
  let node: unknown = api['expect'];
  for (const key of keys) node = isMap(node) ? node[key] : undefined;
  return node !== undefined;
}

/** `check` as one matcher of several on its key: an equality is `eq`. */
function matcher(check: AddedCheck): unknown {
  return check.op === 'equals' ? { eq: literal(check.value.trim()) } : expected(check);
}

/**
 * What the checks on one key write: the one check as it is, else every matcher in one object
 * (all must pass), a later check of the same op replacing an earlier one.
 */
function together(checks: readonly AddedCheck[]): unknown {
  const [only, ...more] = [...new Map(checks.map((check) => [check.op, check])).values()];
  if (only !== undefined && more.length === 0) return expected(only);
  return Object.assign({}, ...checks.map(matcher)) as unknown;
}

/**
 * The edits that add `checks` to the api at `at`: one set per key, with every check on it,
 * after a remove when the api has a check there, since a set never lands on a written node.
 */
export function checkEdits(
  at: string,
  api: WrittenApi,
  checks: readonly AddedCheck[],
): PointerEdit[] {
  const keyed = new Map<string, { keys: string[]; checks: AddedCheck[] }>();
  for (const check of checks) {
    const keys = place(check);
    const pointer = `${at}/expect/${keys.map(token).join('/')}`;
    const group = keyed.get(pointer) ?? { keys, checks: [] };
    keyed.set(pointer, { keys, checks: [...group.checks, check] });
  }
  return [...keyed].flatMap(([pointer, group]): PointerEdit[] => {
    const set: PointerEdit = { pointer, op: 'set', value: together(group.checks) };
    return has(api, group.keys) ? [{ pointer, op: 'remove' }, set] : [set];
  });
}

/** Why `check` will not do, if it will not. */
export function checkProblem(check: AddedCheck): string | undefined {
  const [path, value] = [check.path.trim(), check.value.trim()];
  if (check.kind === 'status' && !/^([1-5]\d\d|[1-5]xx)$/.test(value)) {
    return 'A status is a code such as 201, or a class such as 2xx.';
  }
  if (check.kind === 'body' && path === '') return 'Name the body field, such as $.id.';
  if (check.kind === 'timing' && !(PHASES as readonly string[]).includes(path)) {
    return `Pick a phase: ${PHASES.join(', ')}.`;
  }
  return value === '' && check.op !== 'exists' ? 'Give the value to compare with.' : undefined;
}

/** `check` in a few words: `body $.id exists true`. */
export function checkText(check: AddedCheck): string {
  const path = check.kind === 'status' ? '' : ` ${check.path.trim()}`;
  return `${check.kind}${path} ${check.op} ${check.value.trim()}`;
}

/** The api's own checks, a line per key of its `expect`. */
export function expectLines(api: WrittenApi): string[] {
  const written = api['expect'];
  if (!isMap(written)) return [];
  return Object.entries(written).map(([key, value]) => `${key}: ${text(value)}`);
}

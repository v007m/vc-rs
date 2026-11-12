import { EditOp } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { MASKED, type ConfigValue } from './port.ts';
import { rawVars } from './rawVars.ts';

/** The config file a form edit goes to, and the source `ExplainConfig` names it by. */
export const CONFIG_PATH = 'vibranium.config.yaml';

/** The settings the General section edits, in the order it shows them. */
export const FIELDS = [
  { key: 'defaults.timeout', label: 'Timeout', kind: 'text', hint: '30s, 1m' },
  { key: 'defaults.concurrency', label: 'Concurrency', kind: 'number', hint: '1 or more' },
  { key: 'defaults.perHost', label: 'Per host', kind: 'number', hint: '1 or more' },
] as const;

export type FieldKey = (typeof FIELDS)[number]['key'];

/** One edit as `EditFile` takes it: where, what, and the value it writes. */
export type PointerEdit = Readonly<{
  pointer: string;
  op: 'set' | 'remove' | 'insert' | 'move';
  value?: unknown;
}>;

const OPS = {
  set: EditOp.SET,
  remove: EditOp.REMOVE,
  insert: EditOp.INSERT,
  move: EditOp.MOVE,
} as const;

/** `edits` as `EditFile` and a call's draft take them, each value as JSON text. */
export function wireEdits(edits: readonly PointerEdit[]) {
  return edits.map(({ pointer, op, value }) => ({
    pointer,
    op: OPS[op],
    valueJson: value === undefined ? '' : JSON.stringify(value),
  }));
}

/** A profile as the form edits it; `at` is its place in the file, none for a new one. */
export type DraftProfile = { id: string; isDefault: boolean; at?: number };

/**
 * What the form shows of `vibranium.config.yaml`: the values the file itself sets (no
 * profile's, no `.env`'s), and its profiles in file order, `at` being each one's place there.
 * A var is shown as written, `${env.X}` and all; `resolved` names the ones that could only be
 * shown with their references filled in.
 */
export type ConfigDraft = {
  project: string;
  fields: Record<FieldKey, string>;
  vars: Record<string, string>;
  resolved: string[];
  profiles: DraftProfile[];
};

/** The project as `GetProject` gives it: its name and its profiles in file order. */
export type ProjectNamed = Readonly<{
  name: string;
  profiles: readonly string[];
  defaultProfile?: string;
}>;

/** A JSON pointer token (RFC 6901): `~` and `/` escaped. */
export function token(name: string): string {
  return name.replaceAll('~', '~0').replaceAll('/', '~1');
}

/** The pointer of a dotted settings key: `defaults.timeout` is `/defaults/timeout`. */
export function pointerOf(key: string): string {
  const [head = '', ...rest] = key.split('.');
  return head === 'vars'
    ? `/vars/${token(rest.join('.'))}`
    : `/${key.split('.').map(token).join('/')}`;
}

/** The value the config file sets for `value`'s key, if it sets one. */
function fileValue(value: ConfigValue): string | undefined {
  return value.layers.find((layer) => layer.source === CONFIG_PATH)?.value;
}

/** Each var the file sets, as written where `raw` has it; masked values stay masked. */
function varsOf(set: ReadonlyMap<string, string>, raw: ReadonlyMap<string, string>) {
  const vars: Record<string, string> = {};
  const resolved: string[] = [];
  for (const [key, value] of set) {
    if (!key.startsWith('vars.')) continue;
    const name = key.slice(5);
    const masked = value.includes(MASKED);
    const written = masked ? undefined : raw.get(name);
    vars[name] = written ?? value;
    if (written === undefined && !masked) resolved.push(name);
  }
  return { vars, resolved };
}

/**
 * The draft of the config as `values` (every layer of every key), the project and the file's
 * own `text` show it.
 */
export function draftOf(
  project: ProjectNamed,
  values: readonly ConfigValue[],
  text: string,
): ConfigDraft {
  const set = new Map(
    values.flatMap((value) => {
      const found = fileValue(value);
      return found === undefined ? [] : [[value.key, found] as const];
    }),
  );
  const fields = Object.fromEntries(FIELDS.map(({ key }) => [key, set.get(key) ?? '']));
  const { name, profiles, defaultProfile } = project;
  return {
    project: name,
    fields: fields as Record<FieldKey, string>,
    ...varsOf(set, rawVars(text)),
    profiles: profiles.map((id, at) => ({ id, isDefault: id === defaultProfile, at })),
  };
}

/** `text` as the JSON value a number field writes: a whole number when it reads as one. */
function typed(kind: 'text' | 'number', text: string): string | number {
  return kind === 'number' && /^\d+$/.test(text) ? Number(text) : text;
}

function fieldEdits(base: ConfigDraft, draft: ConfigDraft): PointerEdit[] {
  return FIELDS.flatMap(({ key, kind }): PointerEdit[] => {
    const [was, now] = [base.fields[key], draft.fields[key].trim()];
    if (was === now) return [];
    if (now === '') return [{ pointer: pointerOf(key), op: 'remove' }];
    return [{ pointer: pointerOf(key), op: 'set', value: typed(kind, now) }];
  });
}

function varEdits(base: ConfigDraft, draft: ConfigDraft): PointerEdit[] {
  const removed = Object.keys(base.vars).filter((name) => !(name in draft.vars));
  const changed = Object.entries(draft.vars).filter(
    ([name, value]) => !value.includes(MASKED) && base.vars[name] !== value,
  );
  return [
    ...removed.map((name): PointerEdit => ({ pointer: pointerOf(`vars.${name}`), op: 'remove' })),
    ...changed.map(([name, value]): PointerEdit => ({
      pointer: pointerOf(`vars.${name}`),
      op: 'set',
      value,
    })),
  ];
}

/**
 * The profiles' edits, in an order where each pointer still holds: the default flags first,
 * then removals from the last place up, then new profiles at the end.
 */
function profileEdits(base: ConfigDraft, draft: ConfigDraft): PointerEdit[] {
  const kept = new Map(draft.profiles.flatMap((p) => (p.at === undefined ? [] : [[p.at, p]])));
  const flags = base.profiles.flatMap(({ at = 0, isDefault }): PointerEdit[] => {
    const now = kept.get(at);
    if (now === undefined || now.isDefault === isDefault) return [];
    return [{ pointer: `/profiles/${at}/default`, op: 'set', value: now.isDefault }];
  });
  const removals = base.profiles
    .filter(({ at = 0 }) => !kept.has(at))
    .map(({ at = 0 }): PointerEdit => ({ pointer: `/profiles/${at}`, op: 'remove' }))
    .reverse();
  const added = draft.profiles
    .filter((p) => p.at === undefined)
    .map(({ id, isDefault }): PointerEdit => {
      const value = isDefault ? { id, default: true } : { id };
      return { pointer: '/profiles/-', op: 'insert', value };
    });
  return [...flags, ...removals, ...added];
}

/** The edits that turn the file `base` was read from into `draft`; none when they agree. */
export function editsBetween(base: ConfigDraft, draft: ConfigDraft): PointerEdit[] {
  const project = draft.project.trim();
  const named: PointerEdit[] =
    project !== '' && project !== base.project
      ? [{ pointer: '/project', op: 'set', value: project }]
      : [];
  return [
    ...named,
    ...fieldEdits(base, draft),
    ...varEdits(base, draft),
    ...profileEdits(base, draft),
  ];
}

/** Why `id` cannot name a new profile next to `profiles`, if it cannot. */
export function profileProblem(id: string, profiles: readonly DraftProfile[]): string | undefined {
  if (!/^[a-z][a-z0-9-]*$/.test(id)) return 'Use a lowercase word, such as staging or eu.';
  if (profiles.some((p) => p.id === id)) return `There is a profile ${id} already.`;
  return undefined;
}

/** Why `name` cannot name a new variable next to `vars`, if it cannot. */
export function varProblem(
  name: string,
  vars: Readonly<Record<string, string>>,
): string | undefined {
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name))
    return 'Use letters, digits and _, such as ordersUrl.';
  if (name in vars) return `There is a variable ${name} already.`;
  return undefined;
}

import { MASKED } from './port.ts';

/** One user config setting as the engine reads it: its value masked, as a run's are (D711). */
export type UserSetting = Readonly<{ key: string; value: string; shadowed: boolean }>;

/** A change to one key: a new value, or none to remove the key. */
export type UserEdit = Readonly<{ key: string; value?: string }>;

/** How a row differs from the file: a new key, a new value or a key to remove. */
export type UserChange = 'added' | 'edited' | 'removed';

/** A row of Your settings: the file's setting with any edit staged over it. */
export type UserRow = Readonly<{
  key: string;
  value: string;
  shadowed: boolean;
  masked: boolean;
  change?: UserChange;
}>;

const word = '[A-Za-z][A-Za-z0-9]*';
const nested = new RegExp(`^(defaults|reports)(\\.${word})+$`);
const varKey = /^vars\.[A-Za-z_][A-Za-z0-9_]*$/;

/**
 * Why `key` is not one a client may set in the user config, or undefined when it is: the keys
 * `--set` takes and `vars.<name>` (D712). The engine checks the exact key and the value again.
 */
export function userKeyProblem(key: string): string | undefined {
  if (key === '') return 'Enter a key.';
  if (key === 'profile' || nested.test(key) || varKey.test(key)) return undefined;
  if (key.startsWith('vars.')) {
    return 'A var name starts with a letter or _, followed by letters, digits and _.';
  }
  return 'Use profile, defaults.<name>, reports.<name> or vars.<name>.';
}

/** Whether `value` is the redactor's mask, so its content is never shown or edited. */
export function isMasked(value: string): boolean {
  return value.includes(MASKED);
}

function changed(setting: UserSetting, edit: UserEdit | undefined): UserRow {
  const masked = isMasked(setting.value);
  const row = { ...setting, masked };
  if (!edit) return row;
  if (edit.value === undefined) return { ...row, change: 'removed' };
  return { ...row, value: edit.value, masked: false, change: 'edited' };
}

/** The file's settings with `edits` over them, then each key the edits add, in their order. */
export function userRows(settings: readonly UserSetting[], edits: readonly UserEdit[]): UserRow[] {
  const byKey = new Map(edits.map((edit) => [edit.key, edit]));
  const known = new Set(settings.map((setting) => setting.key));
  const rows = settings.map((setting) => changed(setting, byKey.get(setting.key)));
  const added = edits.filter((edit) => !known.has(edit.key) && edit.value !== undefined);
  return [
    ...rows,
    ...added.map((edit) => ({
      key: edit.key,
      value: edit.value ?? '',
      shadowed: false,
      masked: false,
      change: 'added' as const,
    })),
  ];
}

/** `edits` with `edit` in place of any earlier edit to its key. */
export function withEdit(edits: readonly UserEdit[], edit: UserEdit): UserEdit[] {
  return [...edits.filter((other) => other.key !== edit.key), edit];
}

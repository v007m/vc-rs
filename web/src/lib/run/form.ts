import type { RunInit } from './session.svelte.ts';

/** The report files a run can write (`--report`, D671). */
export const reportFormats = ['ctrf', 'junit', 'html'] as const;
export type ReportFormat = (typeof reportFormats)[number];

/** What the Run dialog holds (D632): the visible fields, then More options. The dialog binds its fields. */
export type RunForm = {
  /** Test or file IDs, globs, paths and `tag:<tag>`, split by spaces or commas; OR joins them. */
  selector: string;
  /** The profile, then any profiles layered over it, in order (later wins). */
  profiles: readonly string[];
  concurrency?: number | undefined;
  bail: boolean;
  /** A git ref: only tests in files changed since it (`--changed`). */
  changed: string;
  seed?: number | undefined;
  repeat?: number | undefined;
  strict: boolean;
  reports: readonly ReportFormat[];
};

export const emptyForm: RunForm = {
  selector: '',
  profiles: [],
  bail: false,
  changed: '',
  strict: false,
  reports: [],
};

type Parts = { ids: string[]; paths: string[]; tags: string[] };

/** The selector's words sorted into IDs (and globs), paths and tags, as `vc run` reads them. */
export function selectorParts(selector: string): Parts {
  const parts: Parts = { ids: [], paths: [], tags: [] };
  for (const word of selector.split(/[\s,]+/).filter(Boolean)) {
    if (word.startsWith('tag:')) parts.tags.push(word.slice(4));
    else if (word.includes('/') || /\.ya?ml$/.test(word)) parts.paths.push(word);
    else parts.ids.push(word);
  }
  return parts;
}

/** The `Run` request for `form`: only the fields it sets, so the engine's defaults hold. */
export function runRequest(form: RunForm): RunInit {
  const { ids, paths, tags } = selectorParts(form.selector);
  const changed = form.changed.trim();
  return {
    selection: { ids, paths, tags, ...(changed ? { changed } : {}) },
    profiles: [...form.profiles],
    bail: form.bail,
    strict: form.strict,
    reports: [...form.reports],
    ...(form.concurrency ? { concurrency: form.concurrency } : {}),
    ...(form.repeat ? { repeat: form.repeat } : {}),
    ...(form.seed !== undefined ? { seed: BigInt(form.seed) } : {}),
  };
}

const quote = (word: string): string =>
  /^[\w.#:/@=,-]+$/.test(word) ? word : `'${word.replace(/'/g, `'\\''`)}'`;

/** The same run as a `vc run` command line, to copy into a terminal or CI. */
export function runCommand(form: RunForm): string {
  const { ids, paths, tags } = selectorParts(form.selector);
  const words = ['vc run', ...[...ids, ...paths].map(quote)];
  const flag = (name: string, value: string | undefined | false) => {
    if (value) words.push(value === name ? name : `${name} ${quote(value)}`);
  };
  if (tags.length) flag('-t', tags.join(','));
  if (form.profiles.length) flag('--profile', form.profiles.join(','));
  flag('--concurrency', form.concurrency ? String(form.concurrency) : undefined);
  flag('--bail', form.bail && '--bail');
  flag('--changed', form.changed.trim());
  flag('--seed', form.seed === undefined ? undefined : String(form.seed));
  flag('--repeat', form.repeat ? String(form.repeat) : undefined);
  flag('--strict', form.strict && '--strict');
  if (form.reports.length) flag('--report', form.reports.join(','));
  return words.join(' ');
}

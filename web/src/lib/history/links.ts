import { resolve } from '$app/paths';
import type { ResolvedPathname } from '$app/types';
import { emptyForm, runCommand } from '#lib/run/form.ts';
import type { RunLine } from './port.ts';

/** Where a stored run opens (D644): a load run on the load screen, any other on `/runs/12`. */
export function runHref(run: Pick<RunLine, 'id' | 'kind'>): ResolvedPathname {
  const { id } = run;
  return run.kind === 'load'
    ? resolve('/runs/load/[id=integer]', { id })
    : resolve('/runs/[id=integer]', { id });
}

/** Compare, `base` against `head` (the newer run). */
export function compareHref(base: number, head: number): ResolvedPathname {
  return resolve('/runs/compare/[base=integer]/[head=integer]', { base, head });
}

/** `5 min ago`, `3 h ago`, `2 d ago`, or the date: when a run started, short. */
export function ago(startedMs: number, now = Date.now()): string {
  const minutes = Math.round((now - startedMs) / 60_000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  if (minutes < 60 * 24) return `${Math.round(minutes / 60)} h ago`;
  if (minutes < 60 * 24 * 7) return `${Math.round(minutes / 1440)} d ago`;
  return new Date(startedMs).toLocaleDateString();
}

/** The `vc run` line that runs `run`'s selection again under its profiles. */
export function lineCommand(run: Pick<RunLine, 'selector' | 'profiles'>): string {
  return runCommand({ ...emptyForm, selector: run.selector, profiles: run.profiles });
}

/** `1.2 s`, `3 min 4 s`: a run's length, short. */
export function span(durationMs: number | undefined): string {
  if (durationMs === undefined) return '—';
  if (durationMs < 1000) return `${Math.round(durationMs)} ms`;
  const s = durationMs / 1000;
  if (s < 60) return `${s.toFixed(1)} s`;
  return `${Math.floor(s / 60)} min ${Math.round(s % 60)} s`;
}

/** The Export menu of a stored run (D636): each copies a command, since the UI serves no files (D320). */
export const exports = [
  { id: 'html', label: 'HTML report', command: (id: number) => `vc report ${id} --format html` },
  { id: 'ctrf', label: 'CTRF JSON', command: (id: number) => `vc report ${id} --format ctrf` },
  { id: 'junit', label: 'JUnit XML', command: (id: number) => `vc report ${id} --format junit` },
  {
    id: 'json',
    label: 'Findings as JSON',
    command: (id: number) => `vc history show ${id} --json`,
  },
  {
    id: 'path',
    label: 'HTML report path',
    command: (id: number) => `.vibranium/reports/${id}/index.html`,
  },
] as const;

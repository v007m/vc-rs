import { createContext } from 'svelte';
import type { LoadSession } from '#lib/load/session.svelte.ts';
import type { EstimatePort } from '#lib/run/estimate.ts';
import type { RunHistory } from './history.svelte.ts';
import type { RunsSeams } from './seams.ts';

/** What the Runs space's screens share (D630): the history, the seams and the Run dialog. */
export class RunsSpace {
  /** The top row's title for the screen open, when the view's is not enough (`Run #12`). */
  title: string | undefined = $state();
  /** Whether **Run…** is open, and the selector it opens with. */
  dialog: { selector: string } | undefined = $state.raw();

  constructor(
    readonly history: RunHistory,
    readonly ports: Readonly<{ seams: RunsSeams; load: LoadSession; estimate: EstimatePort }>,
  ) {}

  /** Opens **Run…**, with `selector` filled in. */
  run(selector = ''): void {
    this.dialog = { selector };
  }
}

export const [getRunsSpace, setRunsSpace] = createContext<RunsSpace>();

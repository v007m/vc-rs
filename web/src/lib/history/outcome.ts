import type { RunOutcome } from './port.ts';

/** The `dot` and `pill` modifier class of each run outcome (controls.css). */
export const outcomeClass: Readonly<Record<RunOutcome, string>> = {
  passed: 'pass',
  failed: 'fail',
  cancelled: 'block',
  unfinished: '',
};

/** Each run outcome as the screen says it. */
export const outcomeLabel: Readonly<Record<RunOutcome, string>> = {
  passed: 'Passed',
  failed: 'Failed',
  cancelled: 'Cancelled',
  unfinished: 'Not finished',
};

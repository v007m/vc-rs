import type { RunOutcome } from './port.ts';

/** The `st-dot` and `st-badge` modifier class of each run outcome (strata/vibranium.css). */
export const outcomeClass: Readonly<Record<RunOutcome, string>> = {
  passed: 'vc-pass',
  failed: 'vc-fail',
  cancelled: 'vc-blocked',
  unfinished: '',
};

/** Each run outcome as the screen says it. */
export const outcomeLabel: Readonly<Record<RunOutcome, string>> = {
  passed: 'Passed',
  failed: 'Failed',
  cancelled: 'Cancelled',
  unfinished: 'Not finished',
};

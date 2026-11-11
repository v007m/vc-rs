import { MASKED } from '#lib/settings/port.ts';
import type { PointerEdit } from '#lib/settings/configEdits.ts';

/** What runs a step: an api, a test, a function or a command (`steps`, R43). */
export const STEP_KINDS = ['call', 'test', 'function', 'command'] as const;
export type StepKind = (typeof STEP_KINDS)[number];

/** How often a stage runs: once, `repeat: N`, once per `forEach` item, or `poll` until. */
export type LoopKind = 'once' | 'repeat' | 'forEach' | 'poll';

export const LOOPS: readonly { id: LoopKind; label: string }[] = [
  { id: 'once', label: 'Once' },
  { id: 'repeat', label: 'Repeat ×N' },
  { id: 'forEach', label: 'For each' },
  { id: 'poll', label: 'Poll until' },
];

/** A stage's loop as the form edits it, each field as typed. */
export type Loop = {
  kind: LoopKind;
  times: string;
  over: string;
  until: string;
  every: string;
  timeout: string;
};

/**
 * One step of a flow test as the form shows it. `at` is its place in the file, none for a new
 * one; `keys` are the loop keys its file writes; a `locked` loop is one the form cannot show
 * (a `poll` that checks more than a status, a masked `forEach`) and is never written.
 */
export type Stage = {
  kind: StepKind;
  target: string;
  name: string;
  uses: readonly string[];
  saves: readonly string[];
  loop: Loop;
  locked: boolean;
  keys: readonly string[];
  at?: number;
};

type Node = Readonly<Record<string, unknown>>;

const LOOP_KEYS = ['forEach', 'repeat', 'poll'] as const;
const DURATION = /^\d+(ms|s|m|h)$/;

function record(value: unknown): Node {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? { ...value } : {};
}

function text(value: unknown): string {
  return typeof value === 'string' ? value : JSON.stringify(value ?? '');
}

function once(): Loop {
  return { kind: 'once', times: '2', over: '', until: '200', every: '1s', timeout: '30s' };
}

/** The loop `step` writes, and whether the form can show it. */
function loopOf(step: Node): { loop: Loop; locked: boolean } {
  const loop = once();
  if (step.forEach !== undefined) {
    const over = text(step.forEach);
    return { loop: { ...loop, kind: 'forEach', over }, locked: over.includes(MASKED) };
  }
  if (step.repeat !== undefined)
    return { loop: { ...loop, kind: 'repeat', times: text(step.repeat) }, locked: false };
  if (step.poll === undefined) return { loop, locked: false };
  const poll = record(step.poll);
  const until = record(poll.until);
  const status = Object.keys(until).length === 1 ? until.status : undefined;
  const fields = { until: text(status), every: text(poll.every), timeout: text(poll.timeout) };
  return { loop: { ...loop, kind: 'poll', ...fields }, locked: typeof status !== 'number' };
}

function stageOf(value: unknown, at: number): Stage {
  const step = record(value);
  const kind = STEP_KINDS.find((k) => step[k] !== undefined) ?? 'call';
  const { loop, locked } = loopOf(step);
  return {
    kind,
    target: text(step[kind]),
    name: typeof step.id === 'string' ? step.id : '',
    uses: Object.keys(record(step.vars)),
    saves: Object.keys(record(step.save)),
    loop,
    locked,
    keys: LOOP_KEYS.filter((key) => step[key] !== undefined),
    at,
  };
}

/** The stages of the test `node` as its file writes it. */
export function stagesOf(node: Node): Stage[] {
  const steps = Array.isArray(node.steps) ? (node.steps as unknown[]) : [];
  return steps.map(stageOf);
}

/** A new stage calling `api` once. */
export function newStage(api: string): Stage {
  const blank = { uses: [], saves: [], locked: false, keys: [] };
  return { kind: 'call', target: api, name: '', loop: once(), ...blank };
}

/** `over` as `forEach` writes it: a JSON list as typed, else a template. */
function overOf(over: string): unknown {
  try {
    const list: unknown = JSON.parse(over);
    return Array.isArray(list) ? list : over;
  } catch {
    return over;
  }
}

/** The loop key and value each kind writes; none for once. */
const ENTRIES: Readonly<Record<LoopKind, (loop: Loop) => [string, unknown] | undefined>> = {
  once: () => undefined,
  repeat: ({ times }) => ['repeat', Number(times)],
  forEach: ({ over }) => ['forEach', overOf(over.trim())],
  poll: ({ until, every, timeout }) => [
    'poll',
    { until: { status: Number(until) }, every, timeout },
  ],
};

function loopEntry(loop: Loop): [string, unknown] | undefined {
  return ENTRIES[loop.kind](loop);
}

function loopProblem(loop: Loop): string | undefined {
  const { kind, times, over, until, every, timeout } = loop;
  if (kind === 'repeat' && !(/^\d+$/.test(times) && Number(times) >= 1)) {
    return 'repeat needs a whole number of times, 1 or more';
  }
  if (kind === 'forEach' && over.trim() === '') {
    return 'for each needs a list such as ["a", "b"] or a template such as ${var.ids}';
  }
  if (kind !== 'poll') return undefined;
  if (!/^[1-5]\d\d$/.test(until)) return 'poll until needs a status such as 200';
  if (!DURATION.test(every) || !DURATION.test(timeout)) {
    return 'poll every and timeout need durations such as 2s or 1m';
  }
  return undefined;
}

/** Why the flow cannot be saved as it stands, if it cannot. */
export function flowProblem(stages: readonly Stage[]): string | undefined {
  if (stages.length === 0) return 'A flow needs at least one stage.';
  for (const [at, stage] of stages.entries()) {
    if (stage.kind === 'call' && stage.target.trim() === '') return `Stage ${at + 1} needs an api.`;
    const problem = stage.locked ? undefined : loopProblem(stage.loop);
    if (problem) return `Stage ${at + 1}: ${problem}.`;
  }
  return undefined;
}

/** The edits that turn the step `before` at `pointer` into `after`. */
function stageEdits(pointer: string, before: Stage, after: Stage): PointerEdit[] {
  const edits: PointerEdit[] = [];
  if (after.kind === 'call' && after.target !== before.target) {
    edits.push({ pointer: `${pointer}/call`, op: 'set', value: after.target });
  }
  const entry = loopEntry(after.loop);
  if (after.locked || JSON.stringify(entry) === JSON.stringify(loopEntry(before.loop)))
    return edits;
  edits.push(
    ...before.keys.map((key) => ({ pointer: `${pointer}/${key}`, op: 'remove' as const })),
  );
  if (entry) edits.push({ pointer: `${pointer}/${entry[0]}`, op: 'set', value: entry[1] });
  return edits;
}

function stepOf(stage: Stage): Node {
  const entry = loopEntry(stage.loop);
  return entry ? { call: stage.target, [entry[0]]: entry[1] } : { call: stage.target };
}

/**
 * The pointer edits that turn the steps of the test at `pointer` from `base` into `stages`:
 * removed steps last first, then each kept step's changes at its place after them, then the
 * new steps at the end. Steps keep their order, so their comments stay with them.
 */
export function flowEdits(
  pointer: string,
  base: readonly Stage[],
  stages: readonly Stage[],
): PointerEdit[] {
  const steps = `${pointer}/steps`;
  const kept = stages.filter((stage) => stage.at !== undefined);
  const keptAt = new Set(kept.map((stage) => stage.at));
  const removed = base.filter((stage) => !keptAt.has(stage.at)).reverse();
  const removes = removed.map((stage) => ({
    pointer: `${steps}/${stage.at}`,
    op: 'remove' as const,
  }));
  const changes = kept.flatMap((stage, at) => {
    const before = base.find((b) => b.at === stage.at);
    return before ? stageEdits(`${steps}/${at}`, before, stage) : [];
  });
  const added = stages.filter((stage) => stage.at === undefined);
  const inserts = added.map((stage) => ({
    pointer: `${steps}/-`,
    op: 'insert' as const,
    value: stepOf(stage),
  }));
  return [...removes, ...changes, ...inserts];
}

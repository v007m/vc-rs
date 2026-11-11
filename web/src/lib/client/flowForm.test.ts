import { describe, expect, it } from 'vitest';
import { MASKED } from '#lib/settings/port.ts';
import { flowEdits, flowProblem, newStage, stagesOf, type Stage } from './flowForm.ts';

const TEST = {
  id: 'createsThenPolls',
  steps: [
    {
      id: 'makesUser',
      call: 'users#create',
      vars: { email: 'a@example.com' },
      save: { userId: '${response.body.id}' },
    },
    { call: 'users#get', poll: { until: { status: 200 }, every: '1s', timeout: '10s' } },
    { command: 'echo seeded', repeat: 3, forEach: '${var.ids}' },
    {
      call: 'users#list',
      poll: { until: { body: { match: { state: 'done' } } }, every: '2s', timeout: '2m' },
    },
  ],
};
const AT = '/tests/0';

describe('stagesOf', () => {
  it('reads each step with what runs it, its vars, saves and loop', () => {
    const [made, polled, seeded, matched] = stagesOf(TEST);
    expect(made).toMatchObject({
      kind: 'call',
      target: 'users#create',
      name: 'makesUser',
      uses: ['email'],
      saves: ['userId'],
    });
    expect(made?.loop.kind).toBe('once');
    expect(polled?.loop).toMatchObject({ kind: 'poll', until: '200', every: '1s', timeout: '10s' });
    expect(seeded).toMatchObject({
      kind: 'command',
      target: 'echo seeded',
      keys: ['forEach', 'repeat'],
    });
    expect(seeded?.loop).toMatchObject({ kind: 'forEach', over: '${var.ids}' });
    expect(matched?.locked).toBe(true);
  });

  it('shows a poll field the step does not write as empty', () => {
    const steps = [{ call: 'jobs#get', poll: { until: { status: 200 }, timeout: '30s' } }];
    const [stage] = stagesOf({ steps });
    expect(stage?.loop).toMatchObject({ kind: 'poll', until: '200', every: '', timeout: '30s' });
  });

  it('locks a forEach whose list reads a secret', () => {
    const [stage] = stagesOf({ steps: [{ call: 'a#b', forEach: [MASKED, 'x'] }] });
    expect(stage?.locked).toBe(true);
  });
});

describe('flowEdits', () => {
  it('is empty while the stages are as written', () => {
    expect(flowEdits(AT, stagesOf(TEST), stagesOf(TEST))).toEqual([]);
  });

  it('removes steps last first, edits kept ones at their new place and adds new ones at the end', () => {
    const stages = stagesOf(TEST);
    stages.splice(0, 1);
    const polled = stages[0];
    if (!polled) throw new Error('no stage');
    polled.loop.kind = 'repeat';
    polled.loop.times = '4';
    const added = newStage('users#delete');
    added.loop = { ...added.loop, kind: 'forEach', over: '["u1", "u2"]' };
    stages.push(added);
    expect(flowEdits(AT, stagesOf(TEST), stages)).toEqual([
      { pointer: '/tests/0/steps/0', op: 'remove' },
      { pointer: '/tests/0/steps/0/poll', op: 'remove' },
      { pointer: '/tests/0/steps/0/repeat', op: 'set', value: 4 },
      {
        pointer: '/tests/0/steps/-',
        op: 'insert',
        value: { call: 'users#delete', forEach: ['u1', 'u2'] },
      },
    ]);
  });

  it('drops every loop key a step writes when its loop changes, and changes the api', () => {
    const stages = stagesOf(TEST);
    const seeded = stages[2];
    const made = stages[0];
    if (!seeded || !made) throw new Error('no stage');
    seeded.loop.kind = 'once';
    made.target = 'users#invite';
    expect(flowEdits(AT, stagesOf(TEST), stages)).toEqual([
      { pointer: '/tests/0/steps/0/call', op: 'set', value: 'users#invite' },
      { pointer: '/tests/0/steps/2/forEach', op: 'remove' },
      { pointer: '/tests/0/steps/2/repeat', op: 'remove' },
    ]);
  });

  it('drops parallel with the loop it runs beside, and keeps it while the step still loops', () => {
    const test = { steps: [{ call: 'users#get', repeat: 3, parallel: 2 }] };
    const switched = (change: (stage: Stage) => void) => {
      const stages = stagesOf(test);
      if (stages[0]) change(stages[0]);
      return flowEdits(AT, stagesOf(test), stages);
    };
    expect(switched((s) => (s.loop.kind = 'once'))).toEqual([
      { pointer: '/tests/0/steps/0/repeat', op: 'remove' },
      { pointer: '/tests/0/steps/0/parallel', op: 'remove' },
    ]);
    expect(switched((s) => (s.loop.kind = 'poll'))).toEqual([
      { pointer: '/tests/0/steps/0/repeat', op: 'remove' },
      { pointer: '/tests/0/steps/0/parallel', op: 'remove' },
      {
        pointer: '/tests/0/steps/0/poll',
        op: 'set',
        value: { until: { status: 200 }, every: '1s', timeout: '30s' },
      },
    ]);
    expect(switched((s) => (s.loop.times = '5'))).toEqual([
      { pointer: '/tests/0/steps/0/repeat', op: 'remove' },
      { pointer: '/tests/0/steps/0/repeat', op: 'set', value: 5 },
    ]);
  });

  it('writes a poll as a status check with its interval and timeout', () => {
    const stages = stagesOf(TEST);
    const polled = stages[1];
    if (!polled) throw new Error('no stage');
    polled.loop.timeout = '30s';
    expect(flowEdits(AT, stagesOf(TEST), stages)).toEqual([
      { pointer: '/tests/0/steps/1/poll', op: 'remove' },
      {
        pointer: '/tests/0/steps/1/poll',
        op: 'set',
        value: { until: { status: 200 }, every: '1s', timeout: '30s' },
      },
    ]);
  });

  it('never writes a locked loop', () => {
    const stages = stagesOf(TEST);
    const matched = stages[3];
    if (!matched) throw new Error('no stage');
    matched.loop.every = '5s';
    expect(flowEdits(AT, stagesOf(TEST), stages)).toEqual([]);
  });
});

describe('flowProblem', () => {
  it('says which stage cannot be saved and why', () => {
    const stage = newStage('users#get');
    expect(flowProblem([stage])).toBeUndefined();
    expect(flowProblem([])).toBe('A flow needs at least one stage.');
    expect(flowProblem([{ ...stage, target: ' ' }])).toBe('Stage 1 needs an api.');
    const loops = [
      [{ kind: 'repeat', times: '0' }, 'Stage 1: repeat needs a whole number of times, 1 or more.'],
      [
        { kind: 'forEach', over: ' ' },
        'Stage 1: for each needs a list such as ["a", "b"] or a template such as ${var.ids}.',
      ],
      [{ kind: 'poll', until: 'ok' }, 'Stage 1: poll until needs a status such as 200.'],
      [
        { kind: 'poll', every: 'soon' },
        'Stage 1: poll every and timeout need durations such as 2s or 1m.',
      ],
    ] as const;
    for (const [loop, said] of loops) {
      expect(flowProblem([{ ...stage, loop: { ...stage.loop, ...loop } }])).toBe(said);
    }
  });

  it('takes a poll duration exactly as the engine reads one: a whole number and one unit', () => {
    const stage = newStage('jobs#get');
    const poll = (every: string) =>
      flowProblem([{ ...stage, loop: { ...stage.loop, kind: 'poll', every } }]);
    for (const every of ['250ms', '2s', '1m', '12h', '0s', '18446744073709551615ms']) {
      expect(poll(every)).toBeUndefined();
    }
    const rejects = ['1m30s', '1h30m', '1.5s', '2', '2S', ' 2s', '-1s', '+2s', '2 s', '1d'];
    for (const every of [...rejects, '18446744073709551616ms', '18446744073709552s']) {
      expect(poll(every)).toBe('Stage 1: poll every and timeout need durations such as 2s or 1m.');
    }
  });
});

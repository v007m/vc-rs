import { describe, expect, it } from 'vitest';
import { MASKED } from '#lib/settings/port.ts';
import { flowEdits, flowProblem, newStage, stagesOf } from './flowForm.ts';

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
});

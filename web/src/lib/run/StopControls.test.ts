import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { CancelLevel } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { events, fakeEngine } from '#lib/testing/engine.ts';
import { RunSession } from './session.svelte.ts';
import StopControls from './StopControls.svelte';

function held(levels: CancelLevel[]): { session: RunSession; begun: Promise<void> } {
  let started!: () => void;
  const begun = new Promise<void>((resolve) => (started = resolve));
  const session = new RunSession(
    fakeEngine({
      async *run(_request, context) {
        yield { event: events({ case: 'runStarted', value: { instances: 1 } })[0] };
        started();
        await new Promise((resolve) => context.signal.addEventListener('abort', resolve));
      },
      cancel({ level }) {
        levels.push(level);
        return {};
      },
    }),
  );
  return { session, begun };
}

describe('StopControls', () => {
  it('stops, then offers Stop now and Skip cleanups while the cleanups run', async () => {
    const levels: CancelLevel[] = [];
    const { session, begun } = held(levels);
    void session.start({});
    await begun;
    render(StopControls, { session });
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(screen.getByRole('status').textContent).toContain('running the cleanups');
    await userEvent.click(screen.getByRole('button', { name: 'Stop now' }));
    await userEvent.click(screen.getByRole('button', { name: 'Skip cleanups' }));
    expect(levels).toEqual([CancelLevel.STOP, CancelLevel.STOP, CancelLevel.ABANDON]);
    expect(screen.getByRole('status').textContent).toContain('Skipping the cleanups');
    expect(screen.queryByRole('button')).toBeNull();
  });
});

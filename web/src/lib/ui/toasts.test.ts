import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import ToastStack from './ToastStack.svelte';
import { TOAST_MS, Toasts } from './toasts.svelte.ts';

/** A timer the test runs by hand: each pending leave with its delay. */
function clock(): {
  timer: (run: () => void, ms: number) => void;
  pending: [() => void, number][];
} {
  const pending: [() => void, number][] = [];
  return { timer: (run, ms) => void pending.push([run, ms]), pending };
}

describe('Toasts', () => {
  it('shows a toast that leaves by itself, an error later', () => {
    const { timer, pending } = clock();
    const toasts = new Toasts(timer);
    toasts.show('Saved schedule nightly.');
    toasts.fail(new Error('Digest changed'));
    expect(toasts.list.map((t) => t.tone)).toEqual(['done', 'error']);
    expect(pending.map(([, ms]) => ms)).toEqual([TOAST_MS.done, TOAST_MS.error]);
    pending[0]?.[0]();
    expect(toasts.list.map((t) => t.text)).toEqual(['Digest changed']);
  });

  it('keeps the newest three', () => {
    const toasts = new Toasts(() => undefined);
    for (const n of [1, 2, 3, 4]) toasts.show(`t${n}`);
    expect(toasts.list.map((t) => t.text)).toEqual(['t2', 't3', 't4']);
  });

  it('renders each as a status, an error as an alert, and runs its action once', async () => {
    const toasts = new Toasts(() => undefined);
    let undone = 0;
    toasts.show('Removed plugin.', 'done', { label: 'Open', run: () => undone++ });
    toasts.show('Write failed', 'error');
    render(ToastStack, { toasts });
    expect(screen.getByRole('alert').textContent).toContain('Write failed');
    await userEvent.click(await screen.findByRole('button', { name: 'Open' }));
    expect(undone).toBe(1);
    expect(screen.queryByText('Removed plugin.')).toBeNull();
  });
});

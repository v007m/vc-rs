import { ConnectError } from '@connectrpc/connect';
import { getContext, setContext } from 'svelte';

/** A toast's kind: news of something done, or an error the person should see. */
export type ToastTone = 'done' | 'error';

/** One action a toast offers (Undo, Open), which also dismisses it. */
export type ToastAction = Readonly<{ label: string; run: () => void }>;

export type Toast = Readonly<{ id: number; text: string; tone: ToastTone; action?: ToastAction }>;

/** How long a toast stays: about 3 s (D604), longer for an error, which takes longer to read. */
export const TOAST_MS: Readonly<Record<ToastTone, number>> = { done: 3200, error: 6000 };

/** At most this many toasts show at once; the oldest goes first. */
const MAX = 3;

type Timer = (run: () => void, ms: number) => unknown;

/**
 * The app's toasts: a capsule at the bottom centre for each write, finished run or error
 * (D604). Each one leaves by itself after its time; `timer` is `setTimeout` outside tests.
 */
export class Toasts {
  list: readonly Toast[] = $state.raw([]);
  #next = 1;
  readonly #timer: Timer;

  constructor(timer: Timer = (run, ms) => setTimeout(run, ms)) {
    this.#timer = timer;
  }

  /** Shows `text`; returns the toast's ID. */
  show(text: string, tone: ToastTone = 'done', action?: ToastAction): number {
    const id = this.#next++;
    const toast: Toast = action ? { id, text, tone, action } : { id, text, tone };
    this.list = [...this.list, toast].slice(-MAX);
    this.#timer(() => this.dismiss(id), TOAST_MS[tone]);
    return id;
  }

  /** Shows what went wrong, in the engine's words for a Connect error. */
  fail(error: unknown): number {
    const text = error instanceof Error ? ConnectError.from(error).rawMessage : String(error);
    return this.show(text, 'error');
  }

  dismiss(id: number): void {
    this.list = this.list.filter((toast) => toast.id !== id);
  }
}

const key = Symbol('toasts');

export function setToasts(toasts: Toasts): Toasts {
  return setContext(key, toasts);
}

/** The app's toasts; outside the shell (a screen alone in a test) a fresh set nobody shows. */
export function getToasts(): Toasts {
  return getContext<Toasts | undefined>(key) ?? new Toasts();
}

import type { ProjectStatus } from '#lib/shell/project.svelte.ts';
import type { ToastAction, Toasts } from '#lib/ui/toasts.svelte.ts';

/** What onboarding needs of the app's project facts: loading them again once a project exists. */
export type Reload = Pick<ProjectStatus, 'load'>;

/** `work`, with any error it throws also shown as a toast; the caller still sees the error. */
export function loud<A extends unknown[], T>(
  toasts: Toasts,
  work: (...args: A) => Promise<T>,
): (...args: A) => Promise<T> {
  return async (...args) => {
    try {
      return await work(...args);
    } catch (error) {
      toasts.fail(error);
      throw error;
    }
  };
}

/** Says what was written, then reloads the project so the app replaces onboarding. */
export async function finish(
  toasts: Toasts,
  project: Reload,
  done: Readonly<{ text: string; action?: ToastAction | undefined }>,
): Promise<void> {
  toasts.show(done.text, 'done', done.action);
  await project.load();
}

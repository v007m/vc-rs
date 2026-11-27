import { ConnectError } from '@connectrpc/connect';
import type { Maybe } from '#lib/api/maybe.ts';
import { Toasts } from '#lib/ui/toasts.svelte.ts';
import type { Staged } from '#lib/writes/staged.svelte.ts';
import type { ScheduleChange, ScheduleLine, SchedulesPort, ScheduleSpec } from './port.ts';

/** The spec a list line was read from, to write it back changed. */
export function specOf(line: ScheduleLine): ScheduleSpec {
  const { id, name, cron, selector, profiles, notify, runner, enabled } = line;
  return { id, ...(name ? { name } : {}), cron, selector, profiles, notify, runner, enabled };
}

/** What a toast says once `change` is written. */
export function doneText(change: ScheduleChange): string {
  return 'put' in change
    ? `Saved schedule ${change.put.id}.`
    : `Removed schedule ${change.remove}.`;
}

/**
 * The Schedules screen's list (D705) and its writes, each a previewed config diff written with
 * the preview's digest (D706, D707). A write that lands shows a toast and reads the list again;
 * a thrown error shows one too, and the `Staged` note keeps it inline.
 */
export class ScheduleList {
  lines: readonly ScheduleLine[] = $state.raw([]);
  loaded = $state(false);
  /** Why the engine has no schedules to list (it lacks the RPCs). */
  reason: string | undefined = $state();
  error: string | undefined = $state();
  /** The schedule whose switch is being written. */
  toggling: string | undefined = $state();
  readonly #port: SchedulesPort;
  readonly #toasts: Toasts;

  constructor(port: SchedulesPort, toasts: Toasts = new Toasts()) {
    this.#port = port;
    this.#toasts = toasts;
  }

  async load(): Promise<void> {
    try {
      const got = await this.#port.list();
      if (got.available) this.lines = got.value;
      this.reason = got.available ? undefined : got.reason;
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.loaded = true;
    }
  }

  /** Previews `change` into `staged`. */
  review(staged: Staged, change: ScheduleChange): Promise<void> {
    return staged.review(() => this.#loud(() => this.#port.preview(change)));
  }

  /** Writes `change` with `staged`'s digest; true once it wrote. */
  async write(staged: Staged, change: ScheduleChange): Promise<boolean> {
    const wrote = await staged.write((digest) =>
      this.#loud(() => this.#port.write(change, digest)),
    );
    if (wrote) await this.#landed(doneText(change));
    return wrote;
  }

  /** Previews schedule `id`'s GitHub Actions workflow into `staged`. */
  reviewExport(staged: Staged, id: string): Promise<void> {
    return staged.review(() => this.#loud(() => this.#port.previewExport(id)));
  }

  /** Writes the workflow with `staged`'s digest; gives its path once written. */
  async writeExport(staged: Staged, id: string): Promise<string | undefined> {
    let path: string | undefined;
    await staged.write((digest) =>
      this.#loud(async () => {
        const got = await this.#port.export(id, digest);
        if (got.available) path = got.value;
        return got;
      }),
    );
    if (path !== undefined) this.#toasts.show(`Wrote ${path}.`);
    return path;
  }

  /** Turns `line` on or off at once: the switch moves now, and back when the write fails. */
  async toggle(line: ScheduleLine, enabled: boolean): Promise<void> {
    if (this.toggling !== undefined) return;
    const change = { put: { ...specOf(line), enabled } };
    this.toggling = line.id;
    this.#mark(line.id, enabled);
    try {
      await this.#writeAtOnce(change);
      await this.#landed(`Turned ${enabled ? 'on' : 'off'} schedule ${line.id}.`);
    } catch (error) {
      this.#mark(line.id, line.enabled);
      this.error = ConnectError.from(error).rawMessage;
      this.#toasts.fail(error);
    } finally {
      this.toggling = undefined;
    }
  }

  /** The dry run and then the write with its digest, with no diff to look at between them. */
  async #writeAtOnce(change: ScheduleChange): Promise<void> {
    const dry = await this.#port.preview(change);
    if (!dry.available) throw new Error(dry.reason);
    const wet = await this.#port.write(change, dry.value.digest);
    if (!wet.available) throw new Error(wet.reason);
  }

  #mark(id: string, enabled: boolean): void {
    this.lines = this.lines.map((line) => (line.id === id ? { ...line, enabled } : line));
  }

  async #landed(text: string): Promise<void> {
    this.#toasts.show(text);
    await this.load();
  }

  /** `work`, with a toast for the error it throws, which it throws on for the `Staged` note. */
  async #loud<T>(work: () => Promise<Maybe<T>>): Promise<Maybe<T>> {
    try {
      return await work();
    } catch (error) {
      this.#toasts.fail(error);
      throw error;
    }
  }
}

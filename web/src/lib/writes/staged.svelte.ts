import { ConnectError } from '@connectrpc/connect';
import type { Maybe } from '#lib/api/maybe.ts';
import { fileChange } from '#lib/editor/port.ts';
import type { FileChange } from '#lib/editor/seams.ts';
import type { FileChange as WireChange } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** A dry run's answer: the files a write would change, and the digest the write sends back. */
export type Preview = Readonly<{ files: readonly FileChange[]; digest: string }>;

/** A wire dry run (`write: false`) as a preview; every write RPC answers `files` and `digest`. */
export function preview(got: { files: readonly WireChange[]; digest: string }): Preview {
  return { files: got.files.map(fileChange), digest: got.digest };
}

/**
 * One preview-then-write (ui.md §5): `review` asks for the dry run and keeps it, `write` sends
 * its digest back, so nothing changed since the preview is overwritten (D653). A step's error,
 * or the reason an engine lacks the RPC, lands in `note`.
 */
export class Staged {
  shown: Preview | undefined = $state.raw();
  busy = $state(false);
  note: string | undefined = $state();

  /** Forgets the preview, as when the person edits what it previewed. */
  reset(): void {
    this.shown = undefined;
    this.note = undefined;
  }

  async review(dry: () => Promise<Maybe<Preview>>): Promise<void> {
    await this.#step(async () => {
      const got = await dry();
      if (got.available) this.shown = got.value;
      else this.note = got.reason;
    });
  }

  /** Writes with the shown preview's digest; true once it wrote. */
  async write(wet: (digest: string) => Promise<Maybe<unknown>>): Promise<boolean> {
    const digest = this.shown?.digest;
    if (digest === undefined) return false;
    let wrote = false;
    await this.#step(async () => {
      const got = await wet(digest);
      if (got.available) {
        wrote = true;
        this.shown = undefined;
      } else this.note = got.reason;
    });
    return wrote;
  }

  async #step(work: () => Promise<void>): Promise<void> {
    this.busy = true;
    this.note = undefined;
    try {
      await work();
    } catch (error) {
      this.note = ConnectError.from(error).rawMessage;
    } finally {
      this.busy = false;
    }
  }
}

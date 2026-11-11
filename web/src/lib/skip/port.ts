import type { Client } from '@connectrpc/connect';
import { fileChange, type ChangesOutput } from '#lib/client/port.ts';
import type { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** A skip to write: `vc skip <target> --reason … [--until …]`. */
export type SkipInput = Readonly<{ target: string; reason: string; until?: string }>;

/** A skip change with the digest of its file as previewed, which its write passes back. */
export type PreviewedChanges = ChangesOutput & Readonly<{ digest: string }>;

/**
 * What skipping asks of the engine. Without a digest it previews the change; with the digest a
 * preview gave, it writes it, and is aborted when the file changed since.
 */
export interface SkipPort {
  skip(input: SkipInput, digest?: string): Promise<PreviewedChanges>;
  unskip(target: string, digest?: string): Promise<PreviewedChanges>;
}

/** The port over the engine client's Skip and Unskip. */
export function enginePort(
  client: Pick<Client<typeof EngineService>, 'skip' | 'unskip'>,
): SkipPort {
  const written = (digest: string | undefined) => ({
    write: digest !== undefined,
    expectedDigest: digest,
  });
  return {
    async skip(input, digest) {
      const {
        files,
        titles,
        digest: previewed,
      } = await client.skip({ ...input, ...written(digest) });
      return { files: files.map(fileChange), titles, digest: previewed };
    },
    async unskip(target, digest) {
      const {
        files,
        titles,
        digest: previewed,
      } = await client.unskip({ target, ...written(digest) });
      return { files: files.map(fileChange), titles, digest: previewed };
    },
  };
}

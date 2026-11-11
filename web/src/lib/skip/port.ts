import type { Client } from '@connectrpc/connect';
import { fileChange, type ChangesOutput } from '#lib/client/port.ts';
import type { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** A skip to write: `vc skip <target> --reason … [--until …]`. */
export type SkipInput = Readonly<{ target: string; reason: string; until?: string }>;

/** What skipping asks of the engine; `write: false` previews the change, `true` writes it. */
export interface SkipPort {
  skip(input: SkipInput, write: boolean): Promise<ChangesOutput>;
  unskip(target: string, write: boolean): Promise<ChangesOutput>;
}

/** The port over the engine client's Skip and Unskip. */
export function enginePort(
  client: Pick<Client<typeof EngineService>, 'skip' | 'unskip'>,
): SkipPort {
  return {
    async skip(input, write) {
      const { files, titles } = await client.skip({ ...input, write });
      return { files: files.map(fileChange), titles };
    },
    async unskip(target, write) {
      const { files, titles } = await client.unskip({ target, write });
      return { files: files.map(fileChange), titles };
    },
  };
}

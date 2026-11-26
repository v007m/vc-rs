import type { Client } from '@connectrpc/connect';
import type { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import type { RunInit } from './session.svelte.ts';

/** What a selection would run: its tests and their files, and the skipped tests it leaves out. */
export type PlanEstimate = Readonly<{ tests: number; files: number; skipped: number }>;

/** Counts what a selection would run, without running it (D642). */
export type EstimatePort = (selection: NonNullable<RunInit['selection']>) => Promise<PlanEstimate>;

/** The estimate over `ListDefinitions`, which takes `vc run`'s selection. */
export function engineEstimate(
  client: Pick<Client<typeof EngineService>, 'listDefinitions'>,
): EstimatePort {
  return async (selection) => {
    const { definitions } = await client.listDefinitions({ selection });
    const ran = definitions.filter((d) => !d.skipped);
    return {
      tests: ran.length,
      files: new Set(ran.map((d) => d.path)).size,
      skipped: definitions.length - ran.length,
    };
  };
}

const counted = (n: number, word: string): string => `${n} ${word}${n === 1 ? '' : 's'}`;

/** The Run dialog's one-line plan: `12 tests · 3 files · 1 skipped`. */
export function planLine({ tests, files, skipped }: PlanEstimate): string {
  if (tests === 0) return 'No test matches. Nothing would run.';
  const parts = [counted(tests, 'test'), counted(files, 'file')];
  if (skipped) parts.push(`${skipped} skipped`);
  return parts.join(' · ');
}

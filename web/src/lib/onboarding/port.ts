import type { Client } from '@connectrpc/connect';
import { unlessMissing, type Maybe } from '#lib/api/maybe.ts';
import type { EngineService, InitResponse } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { preview, type Preview } from '#lib/writes/staged.svelte.ts';

/** The wizard's answers; an empty one leaves `vc init`'s default (D716). */
export type InitChoices = Readonly<{
  name: string;
  service: string;
  baseUrl: string;
  profile: string;
  sample: boolean;
}>;

/** A new project previewed or written: its files and digest, and the folder they go in. */
export type Planned = Preview & Readonly<{ root: string }>;

/**
 * The demo project, previewed or written, and its service: whether it answers at `url`, and
 * the command that starts it. The UI never starts it (D717).
 */
export type DemoPlan = Planned & Readonly<{ running: boolean; url: string; start: string }>;

/**
 * What onboarding asks of the engine. Without `digest` each call previews; with the preview's
 * digest it writes.
 */
export interface OnboardingPort {
  init(choices: InitChoices, digest?: string): Promise<Maybe<Planned>>;
  demo(digest?: string): Promise<Maybe<DemoPlan>>;
}

/** Why an older engine cannot set up a project from here. */
export const initNotYet =
  'This vc cannot set up a project from the browser. Run vc init in a terminal in this folder.';

function planned(got: InitResponse | undefined): Planned {
  return got ? { ...preview(got), root: got.root } : { files: [], digest: '', root: '' };
}

/** The set fields only, so an empty one leaves the engine's default. */
function given(
  choices: InitChoices,
): Partial<Record<'name' | 'service' | 'baseUrl' | 'profile', string>> {
  const keys = ['name', 'service', 'baseUrl', 'profile'] as const;
  return Object.fromEntries(
    keys.flatMap((k) => (choices[k].trim() ? [[k, choices[k].trim()]] : [])),
  );
}

/** The port over the engine client's Init and Demo. */
export function onboardingPort(
  client: Pick<Client<typeof EngineService>, 'init' | 'demo'>,
): OnboardingPort {
  const write = (digest: string | undefined) =>
    digest === undefined ? { write: false } : { write: true, expectedDigest: digest };
  return {
    init: (choices, digest) =>
      unlessMissing(initNotYet, async () =>
        planned(await client.init({ ...given(choices), sample: choices.sample, ...write(digest) })),
      ),
    demo: (digest) =>
      unlessMissing(initNotYet, async () => {
        const { project, running, url, start } = await client.demo(write(digest));
        return { ...planned(project), running, url, start };
      }),
  };
}

/** The last folder of `root` (`~/dev/shop` → `shop`), for the toast. */
export function folderName(root: string): string {
  return root.split('/').filter(Boolean).at(-1) ?? root;
}

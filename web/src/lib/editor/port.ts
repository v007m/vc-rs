import type { Client } from '@connectrpc/connect';
import { unlessMissing } from '#lib/api/maybe.ts';
import type {
  EngineService,
  FileChange as WireChange,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { engineAssist } from './assist.ts';
import {
  explainNotYet,
  filesNotYet,
  fixesNotYet,
  planNotYet,
  type EditorSeams,
  type ExplainPort,
  type FileChange,
  type FileOps,
  type FixPort,
  type PlanPort,
  type Template,
} from './seams.ts';

/** The part of the engine client the editor's seams call. */
export type SeamsClient = Pick<
  Client<typeof EngineService>,
  | 'complete'
  | 'hover'
  | 'explain'
  | 'applyFixes'
  | 'plan'
  | 'createFile'
  | 'rename'
  | 'format'
  | 'fileReferences'
  | 'trashFile'
>;

/** A wire file change as the editor shows it; a new file's `before` is empty. */
export function fileChange(change: WireChange): FileChange {
  return {
    path: change.path,
    before: change.before ?? '',
    after: change.after,
    ...(change.movedFrom ? { movedFrom: change.movedFrom } : {}),
  };
}

/** `CreateFile`'s kind and file ID for a template (D653). */
export function newFile(template: Template, name: string, service: string): [string, string] {
  if (template === 'service') return ['api', name];
  return [template, `${service}.${name}`];
}

/** `vc new file`: a preview, then the write with the preview's digest. */
function create(client: SeamsClient): FileOps['create'] {
  return (template, name, service) =>
    unlessMissing(filesNotYet, async () => {
      const [kind, id] = newFile(template, name, service);
      const { digest } = await client.createFile({ kind, id, write: false });
      const made = await client.createFile({ kind, id, write: true, expectedDigest: digest });
      return made.path;
    });
}

function fileOps(client: SeamsClient): FileOps {
  return {
    create: create(client),
    previewRename: (from, to) =>
      unlessMissing(filesNotYet, async () => {
        const { files, digest } = await client.rename({ from, to, write: false });
        return { files: files.map(fileChange), digest };
      }),
    rename: (from, to, digest) =>
      unlessMissing(filesNotYet, async () => {
        const done = await client.rename({ from, to, write: true, expectedDigest: digest });
        return done.files.map(fileChange);
      }),
    format: (path, text) =>
      unlessMissing(filesNotYet, async () => (await client.format({ path, text })).text),
    references: (path) =>
      unlessMissing(filesNotYet, async () => {
        const { references, unread } = await client.fileReferences({ path });
        return { uses: references.map((r) => `${r.from} · ${r.path}:${r.line}`), unread };
      }),
    trash: (path, digest) =>
      unlessMissing(filesNotYet, async () => {
        const { trashedTo } = await client.trashFile({ path, expectedDigest: digest });
        return trashedTo;
      }),
  };
}

function planPort(client: SeamsClient): PlanPort {
  return {
    plan: (paths) =>
      unlessMissing(planNotYet, async () => {
        const got = await client.plan({ selection: { paths: [...paths] } });
        return { line: got.line, tree: got.tree, instances: got.instances, notices: got.notices };
      }),
  };
}

function explainPort(client: SeamsClient): ExplainPort {
  return {
    explain: (target) =>
      unlessMissing(explainNotYet, async () => {
        const { text, markdown, notices } = await client.explain({ target });
        return { text, markdown, notices };
      }),
  };
}

/** `ApplyFixes` as a preview of `path` alone: the text after its fixes, or none to make. */
function fixPort(client: SeamsClient): FixPort {
  return {
    fix: (path, codes) =>
      unlessMissing(fixesNotYet, async () => {
        const { files } = await client.applyFixes({ codes: [...codes], paths: [path] });
        return files.find((f) => f.path === path)?.after;
      }),
  };
}

/** The editor's seams over the engine client (ui.md §10 rows 4–6). */
export function engineEditorSeams(client: SeamsClient): EditorSeams {
  return {
    assist: engineAssist(client),
    files: fileOps(client),
    plan: planPort(client),
    explain: explainPort(client),
    fixes: fixPort(client),
  };
}

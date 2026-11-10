import type { Definition } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { toRow } from '#lib/tests/catalog.ts';
import type { Group } from '#lib/tree/tree.ts';

/** The apis grouped by api file, for the Apis tree; `filter` matches an ID or a name. */
export function apiGroups(definitions: readonly Definition[], filter = ''): Group[] {
  const needle = filter.trim().toLowerCase();
  const groups = new Map<string, Group['leaves'][number][]>();
  for (const row of definitions.map(toRow)) {
    if (needle && !`${row.id} ${row.name}`.toLowerCase().includes(needle)) continue;
    const leaves = groups.get(row.fileId) ?? [];
    leaves.push({ key: row.id, label: row.name, title: row.id });
    groups.set(row.fileId, leaves);
  }
  return [...groups].map(([fileId, leaves]) => ({ key: `${fileId}#`, label: fileId, leaves }));
}

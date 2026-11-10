import type { Group, Leaf } from '#lib/tree/tree.ts';
import type { ApiInfo } from './port.ts';

/** The apis grouped by api file, for the Apis tree; `filter` matches an ID or a name. */
export function apiGroups(apis: readonly ApiInfo[], filter = ''): Group[] {
  const needle = filter.trim().toLowerCase();
  const groups = new Map<string, Leaf[]>();
  for (const api of apis) {
    if (needle && !`${api.id} ${api.name}`.toLowerCase().includes(needle)) continue;
    const fileId = api.id.split('#')[0] ?? api.id;
    const leaves = groups.get(fileId) ?? [];
    leaves.push({ key: api.id, label: api.name, title: api.id });
    groups.set(fileId, leaves);
  }
  return [...groups].map(([fileId, leaves]) => ({ key: `${fileId}#`, label: fileId, leaves }));
}

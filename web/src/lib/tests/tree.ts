import type { FileGroup } from './catalog.ts';

/** A node of the Tests tree: a file, or a test in it. */
export type TreeNode = Readonly<
  { kind: 'file'; key: string; group: FileGroup } | { kind: 'test'; key: string; parent: string }
>;

/** The tree key of a file node; test nodes use the test's full ID, which always has a `#`. */
export function fileKey(fileId: string): string {
  return `file:${fileId}`;
}

/** The nodes a keyboard user can move through: every file, and the tests of open files. */
export function visibleNodes(
  groups: readonly FileGroup[],
  closed: ReadonlySet<string>,
): TreeNode[] {
  return groups.flatMap((group) => {
    const key = fileKey(group.fileId);
    const file: TreeNode = { kind: 'file', key, group };
    if (closed.has(key)) return [file];
    return [file, ...group.tests.map((t): TreeNode => ({ kind: 'test', key: t.id, parent: key }))];
  });
}

/** What a key press does in the tree: move focus, or open or close a file. */
export type TreeMove = Readonly<
  { kind: 'focus'; key: string } | { kind: 'open'; key: string } | { kind: 'close'; key: string }
>;

type Step = (
  nodes: readonly TreeNode[],
  at: number,
  closed: ReadonlySet<string>,
) => TreeMove | undefined;

function focusAt(nodes: readonly TreeNode[], index: number): TreeMove | undefined {
  const node = nodes[index];
  return node && { kind: 'focus', key: node.key };
}

/** The WAI-ARIA tree view keys. */
const steps: Record<string, Step> = {
  ArrowDown: (nodes, at) => focusAt(nodes, at + 1),
  ArrowUp: (nodes, at) => focusAt(nodes, at - 1),
  Home: (nodes) => focusAt(nodes, 0),
  End: (nodes) => focusAt(nodes, nodes.length - 1),
  ArrowRight: (nodes, at, closed) => {
    const node = nodes[at];
    if (node?.kind !== 'file') return undefined;
    return closed.has(node.key) ? { kind: 'open', key: node.key } : focusAt(nodes, at + 1);
  },
  ArrowLeft: (nodes, at, closed) => {
    const node = nodes[at];
    if (node?.kind === 'test') return { kind: 'focus', key: node.parent };
    return node && !closed.has(node.key) ? { kind: 'close', key: node.key } : undefined;
  },
};

/** The move `key` makes from the node `current`, or `undefined` when the key does nothing. */
export function treeMove(
  nodes: readonly TreeNode[],
  current: string | undefined,
  key: string,
  closed: ReadonlySet<string>,
): TreeMove | undefined {
  return steps[key]?.(
    nodes,
    Math.max(
      0,
      nodes.findIndex((n) => n.key === current),
    ),
    closed,
  );
}

/** A branch of a two-level tree, such as a file, and the keys of the leaves under it. */
export type Branch = Readonly<{ key: string; leaves: readonly string[] }>;

/** A node a keyboard user can reach: a branch, or a leaf under an open branch. */
export type TreeNode = Readonly<
  { kind: 'branch'; key: string } | { kind: 'leaf'; key: string; parent: string }
>;

/** Every branch, and the leaves of the branches that are not closed, in order. */
export function visibleNodes(branches: readonly Branch[], closed: ReadonlySet<string>): TreeNode[] {
  return branches.flatMap(({ key, leaves }) => {
    const branch: TreeNode = { kind: 'branch', key };
    if (closed.has(key)) return [branch];
    return [branch, ...leaves.map((leaf): TreeNode => ({ kind: 'leaf', key: leaf, parent: key }))];
  });
}

/** What a key press does in the tree: move focus, or open or close a branch. */
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
    if (node?.kind !== 'branch') return undefined;
    return closed.has(node.key) ? { kind: 'open', key: node.key } : focusAt(nodes, at + 1);
  },
  ArrowLeft: (nodes, at, closed) => {
    const node = nodes[at];
    if (node?.kind === 'leaf') return { kind: 'focus', key: node.parent };
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
  const at = Math.max(
    0,
    nodes.findIndex((n) => n.key === current),
  );
  return steps[key]?.(nodes, at, closed);
}

/** A leaf as a group tree shows it. */
export type Leaf = Readonly<{ key: string; label: string; title?: string }>;

/** A branch as a group tree shows it: its label and its leaves. */
export type Group = Readonly<{ key: string; label: string; leaves: readonly Leaf[] }>;

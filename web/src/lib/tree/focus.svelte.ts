import { tick } from 'svelte';
import { SvelteSet } from 'svelte/reactivity';
import { treeMove, visibleNodes, type Branch } from './tree.ts';

/** The keys that act on a leaf instead of moving. */
export type LeafKey = ' ' | 'Enter';

/** What a tree component tells its focus model. */
export type TreeHost = Readonly<{
  branches: () => readonly Branch[];
  /** The tree element; its items carry their key in `data-key`. */
  root: () => HTMLElement | undefined;
  onleaf: (key: string, press: LeafKey) => void;
}>;

/**
 * The roving focus and open branches of a WAI-ARIA tree: one item is in the tab order, the
 * arrow keys move between items, and Space or Enter act on a leaf.
 */
export class TreeFocus {
  readonly closed = new SvelteSet<string>();
  focusKey: string | undefined = $state();
  readonly #host: TreeHost;
  readonly nodes = $derived(visibleNodes(this.#branches(), this.closed));

  constructor(host: TreeHost) {
    this.#host = host;
  }

  #branches(): readonly Branch[] {
    return this.#host.branches();
  }

  /** The item in the tab order: the focused one while it is shown, else the first. */
  get tabKey(): string | undefined {
    const key = this.focusKey;
    return this.nodes.some((n) => n.key === key) ? key : this.nodes[0]?.key;
  }

  isOpen(key: string): boolean {
    return !this.closed.has(key);
  }

  setOpen(key: string, open: boolean): void {
    if (open) this.closed.delete(key);
    else this.closed.add(key);
  }

  /** Moves focus to the item `key` once it is rendered. */
  async focus(key: string): Promise<void> {
    this.focusKey = key;
    await tick();
    const items = this.#host.root()?.querySelectorAll<HTMLElement>('[data-key]') ?? [];
    [...items].find((item) => item.dataset.key === key)?.focus();
  }

  /** The key handler for every tree item. */
  readonly onkeydown = (event: KeyboardEvent): void => {
    if (event.target !== event.currentTarget) return;
    const key = (event.currentTarget as HTMLElement).dataset.key;
    const node = this.nodes.find((n) => n.key === key);
    if (node?.kind === 'leaf' && (event.key === ' ' || event.key === 'Enter')) {
      event.preventDefault();
      this.#host.onleaf(node.key, event.key);
      return;
    }
    const move = treeMove(this.nodes, key, event.key, this.closed);
    if (!move) return;
    event.preventDefault();
    if (move.kind === 'focus') void this.focus(move.key);
    else this.setOpen(move.key, move.kind === 'open');
  };
}

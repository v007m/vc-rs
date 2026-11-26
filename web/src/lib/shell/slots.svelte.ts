import { getContext, setContext, type Snippet } from 'svelte';

/** What a screen puts in the shell around it (D537); every part is optional. */
export type SlotFill = Readonly<{
  /** The sidebar's body, in place of the space's view list. */
  sidebar?: Snippet;
  /** The top row's left side, in place of the title and run status. */
  top?: Snippet;
  /** Whether the screen draws its own cards on the stage instead of the shell's one L1 card. */
  bare?: boolean;
}>;

/**
 * The shell's slots that the current screen fills. A screen fills them while it is mounted and
 * empties them when it unmounts, so another screen always starts from the shell's defaults.
 */
export class ShellSlots {
  sidebar: Snippet | undefined = $state.raw();
  top: Snippet | undefined = $state.raw();
  bare = $state(false);

  /** Fills the slots and returns the function that empties them again. */
  fill(fill: SlotFill): () => void {
    this.sidebar = fill.sidebar;
    this.top = fill.top;
    this.bare = fill.bare ?? false;
    return () => {
      if (this.sidebar === fill.sidebar) this.sidebar = undefined;
      if (this.top === fill.top) this.top = undefined;
      this.bare = false;
    };
  }
}

const key = Symbol('shell-slots');

export function setShellSlots(slots: ShellSlots): ShellSlots {
  return setContext(key, slots);
}

/** The shell's slots; outside the shell (a screen rendered alone in a test) a fresh, unused set. */
export function getShellSlots(): ShellSlots {
  return getContext<ShellSlots | undefined>(key) ?? new ShellSlots();
}

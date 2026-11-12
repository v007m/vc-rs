/**
 * The install commands, in one place. The hero, the install page and the quickstart all read
 * them, so a change to the tap or the formula is a change here only.
 */

/** Homebrew, through the project's own tap (D45). `--HEAD` until the first tagged release. */
export const BREW_INSTALL = 'brew install --HEAD v007m/tap/vibranium';

/** What the Homebrew command does, in one sentence. */
export const BREW_NOTE =
  'Builds from source with Homebrew’s Rust, so it takes a few minutes, and installs `vc`, `vibranium` (a link to `vc`) and `vc-demo`.';

/** The fallback: Cargo straight from Git (Rust 1.99 or later). */
export const CARGO_INSTALL = 'cargo install --git https://github.com/v007m/vc-rs vc-cli vc-demo --locked';

/** What the Cargo command does, in one sentence. */
export const CARGO_NOTE = 'Builds `vc`, `vibranium` and `vc-demo` into `~/.cargo/bin` with your own Rust 1.99 or later.';

export type InstallMethod = 'brew' | 'cargo';

export const INSTALL: Record<InstallMethod, { label: string; command: string; note: string }> = {
  brew: { label: 'Homebrew', command: BREW_INSTALL, note: BREW_NOTE },
  cargo: { label: 'Cargo', command: CARGO_INSTALL, note: CARGO_NOTE },
};

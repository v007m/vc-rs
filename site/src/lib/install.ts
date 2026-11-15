/**
 * The install commands, in one place. The hero, the install page and the quickstart all read
 * them, so a change to the tap or the formula is a change here only.
 */

/** Homebrew, through the project's own tap (D45): the stable release. `--HEAD` builds `master`. */
export const BREW_INSTALL = 'brew install v007m/tap/vibranium';

/** What the Homebrew command does, in one sentence. */
export const BREW_NOTE =
  'Builds from source with Homebrew’s Rust and Node, so it takes a few minutes, and installs `vc` with its web UI, `vibranium` (a link to `vc`) and `vc-demo`.';

/** The fallback: Cargo straight from Git, at the release tag (Rust 1.99 or later). */
export const CARGO_INSTALL =
  'cargo install --git https://github.com/v007m/vc-rs --tag v0.1.0 vc-cli vc-demo --locked';

/** What the Cargo command does, in one sentence. */
export const CARGO_NOTE =
  'Builds `vc`, `vibranium` and `vc-demo` into `~/.cargo/bin` with your own Rust 1.99 or later. This `vc` has no web UI (`vc ui`): build it from a clone for that.';

export type InstallMethod = 'brew' | 'cargo';

export const INSTALL: Record<InstallMethod, { label: string; command: string; note: string }> = {
  brew: { label: 'Homebrew', command: BREW_INSTALL, note: BREW_NOTE },
  cargo: { label: 'Cargo', command: CARGO_INSTALL, note: CARGO_NOTE },
};

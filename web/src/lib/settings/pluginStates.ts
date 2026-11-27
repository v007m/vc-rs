/** One run of a hint: plain text, or a command shown as code. */
export type HintPart = Readonly<{ text: string; code: boolean }>;

/** What the Plugins page shows for a plugin's state (D740): a word, its tone and a hint. */
export type StateView = Readonly<{
  label: string;
  tone: 'vc-pass' | 'vc-blocked';
  hint: HintPart[];
}>;

type Plugin = Readonly<{ id: string; source: string; state: string; detail: string }>;

/** Splits text at backticks: every second run is a command. */
export function hintParts(text: string): HintPart[] {
  return text
    .split('`')
    .map((run, i) => ({ text: run, code: i % 2 === 1 }))
    .filter((part) => part.text !== '');
}

function capital(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

type Words = Readonly<{ label: string; hint?: (plugin: Plugin) => string }>;

/** Each engine state word's words; an unknown word falls back to the engine's own detail. */
const STATES: Readonly<Record<string, Words>> = {
  ready: { label: 'Ready' },
  notApproved: {
    label: 'Not approved',
    hint: (p) => `Not approved yet. Run \`vc plugin add ${p.source}\` to approve it.`,
  },
  notInstalled: {
    label: 'Not installed',
    hint: (p) => `Not installed. Run \`vc plugin add ${p.source}\` to install it again.`,
  },
  asksForMore: {
    label: 'Asks for more',
    hint: (p) =>
      `It asks to reach more than the lock grants. Run \`vc plugin add ${p.source}\` to review it.`,
  },
  notInConfig: {
    label: 'Not in the config',
    hint: (p) => `Only the lock has it. Run \`vc plugin remove ${p.id}\` to remove it.`,
  },
  changed: {
    label: 'Changed',
    hint: (p) =>
      `Its files changed since the lock approved them. Run \`vc plugin add ${p.source}\` again.`,
  },
  noEntry: { label: 'No entry program' },
};

/** The words for a plugin's state; a state without its own hint shows the engine's detail. */
export function stateView(plugin: Plugin): StateView {
  const words = STATES[plugin.state];
  const tone = plugin.state === 'ready' ? 'vc-pass' : 'vc-blocked';
  const fallback = plugin.detail && plugin.detail !== plugin.state ? capital(plugin.detail) : '';
  const hint = words?.hint?.(plugin) ?? (plugin.state === 'ready' ? '' : fallback);
  const label = words?.label ?? capital(plugin.state.replace(/([A-Z])/g, ' $1').toLowerCase());
  return { label, tone, hint: hintParts(hint) };
}

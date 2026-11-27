import { choiceLabels, type Appearance, type Theme } from './appearance.ts';
import type { PaletteItem } from './palette.ts';

/** What the palette's actions need: where things are, and the setters they call. */
export type CommandContext = Readonly<{
  /** The All runs screen with its Run dialog asked for (`/runs?run=ask`). */
  runHref: string;
  /** The editor opened on `path`. */
  fileHref: (path: string) => string;
  files: readonly string[];
  profiles: readonly string[];
  profile: string | undefined;
  appearance: Appearance;
  useProfile: (profile: string) => void;
  setAppearance: (change: Partial<Appearance>) => void;
  /** Says what an action did (a toast). */
  say: (text: string) => void;
}>;

const themes: readonly Theme[] = ['light', 'dark', 'system'];

function actions(ctx: CommandContext): PaletteItem[] {
  return [{ id: 'do:run', label: 'Run tests…', group: 'Actions', href: ctx.runHref }];
}

function profileItems(ctx: CommandContext): PaletteItem[] {
  return ctx.profiles
    .filter((profile) => profile !== ctx.profile)
    .map((profile) => ({
      id: `profile:${profile}`,
      label: `Use profile ${profile}`,
      group: 'Profile',
      run: () => {
        ctx.useProfile(profile);
        ctx.say(`Using profile ${profile} for this session.`);
      },
    }));
}

function appearanceItems(ctx: CommandContext): PaletteItem[] {
  const { theme, density, motion } = ctx.appearance;
  const set = (change: Partial<Appearance>, said: string) => () => {
    ctx.setAppearance(change);
    ctx.say(said);
  };
  const dense = density === 'compact' ? 'comfortable' : 'compact';
  const still = motion === 'reduced' ? 'system' : 'reduced';
  return [
    ...themes
      .filter((t) => t !== theme)
      .map((t) => ({
        id: `theme:${t}`,
        label: `Theme: ${choiceLabels[t] ?? t}`,
        group: 'Appearance',
        run: set({ theme: t }, `Theme: ${choiceLabels[t] ?? t}.`),
      })),
    {
      id: `density:${dense}`,
      label: `Density: ${choiceLabels[dense] ?? dense}`,
      group: 'Appearance',
      run: set({ density: dense }, `Density: ${choiceLabels[dense] ?? dense}.`),
    },
    {
      id: `motion:${still}`,
      label: still === 'reduced' ? 'Reduce motion' : 'Motion: match the system',
      group: 'Appearance',
      run: set(
        { motion: still },
        still === 'reduced' ? 'Motion reduced.' : 'Motion follows the system.',
      ),
    },
  ];
}

function fileItems(ctx: CommandContext): PaletteItem[] {
  return ctx.files.map((path) => ({
    id: `file:${path}`,
    label: path,
    group: 'Files',
    href: ctx.fileHref(path),
    keywords: 'open file',
    searchOnly: true,
  }));
}

/**
 * The palette's actions after its screens (D620): run, switch profile, theme, density and
 * motion, then every project file to open in the editor.
 */
export function commandItems(ctx: CommandContext): PaletteItem[] {
  return [...actions(ctx), ...profileItems(ctx), ...appearanceItems(ctx), ...fileItems(ctx)];
}

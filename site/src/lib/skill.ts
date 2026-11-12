import skillRaw from '../../../skills/vibranium/SKILL.md?raw';
import syntaxRaw from '../../../skills/vibranium/reference/syntax.md?raw';

/**
 * The agent skill in `skills/vibranium/`, the single source. The site serves its files as they
 * are at stable URLs, so an agent can be pointed at one and a person can curl them into place.
 */
const SITE = (import.meta.env.SITE ?? '').replace(/\/$/, '');
const BASE = import.meta.env.BASE_URL.replace(/\/$/, '');

/** Each published file: its path on the site, its path in the skill folder, its bytes. */
export const SKILL_FILES = [
  { route: 'skill.md', file: 'SKILL.md', text: skillRaw },
  { route: 'skill/reference/syntax.md', file: 'reference/syntax.md', text: syntaxRaw },
] as const;

/** The site path of a published skill file (`/vc-rs/skill.md`). */
export const skillPath = (route: string): string => `${BASE}/${route}`;

/** The absolute URL of a published skill file. */
export const skillUrl = (route: string): string => `${SITE}${skillPath(route)}`;

/** The URL to point an agent at. */
export const SKILL_URL = skillUrl('skill.md');

/** Where Claude Code looks for a user skill. */
export const SKILL_DIR = '~/.claude/skills/vibranium';

/** One curl that writes both files into a skill folder, creating it. */
export const skillInstall = (dir: string): string =>
  [
    'curl -fsSL --create-dirs',
    ...SKILL_FILES.map(({ route, file }) => `-o ${dir}/${file} ${skillUrl(route)}`),
  ].join(' ');

/** The install for every project: the user skill folder. */
export const SKILL_INSTALL = skillInstall(SKILL_DIR);

/** What to tell an agent that fetches URLs. */
export const SKILL_PROMPT = `Read ${SKILL_URL} and follow it to write and run Vibranium tests in this project.`;

/** A markdown response, as the raw file is. */
export function markdownResponse(text: string): Response {
  return new Response(text, { headers: { 'Content-Type': 'text/markdown; charset=utf-8' } });
}

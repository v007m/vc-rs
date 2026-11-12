import type { APIRoute } from 'astro';
import { SKILL_FILES, markdownResponse } from '~/lib/skill';

/** `skills/vibranium/SKILL.md`, byte for byte. */
export const GET: APIRoute = () => markdownResponse(SKILL_FILES[0].text);

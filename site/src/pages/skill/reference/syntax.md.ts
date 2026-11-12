import type { APIRoute } from 'astro';
import { SKILL_FILES, markdownResponse } from '~/lib/skill';

/** `skills/vibranium/reference/syntax.md`, byte for byte. */
export const GET: APIRoute = () => markdownResponse(SKILL_FILES[1].text);

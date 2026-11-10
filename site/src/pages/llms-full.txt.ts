import type { APIRoute } from 'astro';
import { INTRO, plainBody, schemaSection, sections, textResponse, urlOf } from '~/lib/llms';

export const GET: APIRoute = async () => {
  const parts = [INTRO, schemaSection()];
  for (const s of await sections()) {
    for (const d of s.docs) {
      const desc = d.data.description ? `\n\n> ${d.data.description}` : '';
      parts.push(`---\n\n# ${d.data.title}\n\nSource: ${urlOf(d)}${desc}\n\n${plainBody(d)}\n`);
    }
  }
  return textResponse(parts.join('\n'));
};

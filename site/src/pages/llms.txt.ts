import type { APIRoute } from 'astro';
import { INTRO, SITE, schemaSection, sections, textResponse, urlOf } from '~/lib/llms';

export const GET: APIRoute = async () => {
  const lines = [INTRO, `The full text of every page is at ${SITE}${import.meta.env.BASE_URL.replace(/\/$/, '')}/llms-full.txt.`, '', schemaSection()];
  for (const s of await sections()) {
    lines.push(`## ${s.label}`, '');
    for (const d of s.docs) {
      const desc = d.data.description ? `: ${d.data.description}` : '';
      lines.push(`- [${d.data.title}](${urlOf(d)})${desc}`);
    }
    lines.push('');
  }
  return textResponse(lines.join('\n'));
};

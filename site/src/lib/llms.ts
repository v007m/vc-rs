import { getCollection, type CollectionEntry } from 'astro:content';
import { KINDS, SCHEMA_BASE, schemaDescription, schemaUrl } from '~/lib/schemas';

type Doc = CollectionEntry<'docs'>;

export const SITE = (import.meta.env.SITE ?? '').replace(/\/$/, '');
const BASE = import.meta.env.BASE_URL.replace(/\/$/, '');

const SECTIONS: Array<[prefix: string, label: string]> = [
  ['start', 'Start here'],
  ['playground', 'Playground'],
  ['walkthroughs', 'Walkthroughs'],
  ['reference/syntax', 'Syntax reference'],
  ['reference/config', 'Config'],
  ['reference/templates', 'Templates'],
  ['reference/pipes', 'Pipes'],
  ['reference/matchers', 'Matchers'],
  ['reference/lifecycle', 'Lifecycle'],
  ['reference/data', 'Data'],
  ['reference/auth', 'Auth'],
  ['reference/load', 'Load testing'],
  ['reference/plugins', 'Plugins and alerts'],
  ['reference/cli', 'CLI'],
  ['guides', 'Guides'],
  ['agents', 'For AI agents'],
  ['reference/diagnostics', 'Diagnostics'],
  ['reference', 'Reference'],
];

function sectionOf(id: string): number {
  const i = SECTIONS.findIndex(([p]) => id === p || id.startsWith(`${p}/`));
  return i === -1 ? SECTIONS.length : i;
}

function orderOf(doc: Doc): number {
  return doc.data.sidebar?.order ?? Number.MAX_SAFE_INTEGER;
}

export function urlOf(doc: Doc): string {
  const id = doc.id.replace(/(^|\/)index$/, '');
  return `${SITE}${BASE}/${id ? `${id}/` : ''}`;
}

export interface Section {
  label: string;
  docs: Doc[];
}

export async function sections(): Promise<Section[]> {
  const docs = (await getCollection('docs')).filter((d) => !d.data.draft && d.id !== 'index');
  docs.sort(
    (a, b) => sectionOf(a.id) - sectionOf(b.id) || orderOf(a) - orderOf(b) || a.id.localeCompare(b.id),
  );
  const out: Section[] = [];
  for (const d of docs) {
    const label = SECTIONS[sectionOf(d.id)]?.[1] ?? 'Other';
    const last = out.at(-1);
    if (last?.label === label) last.docs.push(d);
    else out.push({ label, docs: [d] });
  }
  return out;
}

export function plainBody(doc: Doc): string {
  return (doc.body ?? '')
    .replace(/^import\s.+?from\s+['"].+?['"];?\s*$/gm, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

export function textResponse(body: string): Response {
  return new Response(body, { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
}

export const INTRO = [
  '# Vibranium',
  '',
  '> Vibranium (`vc`) is one Rust engine for functional, contract and load tests of HTTP APIs. Tests are plain YAML files in git. A CLI, a terminal UI, a web UI and an MCP server for AI agents all run on the same engine.',
  '',
].join('\n');

export function schemaSection(): string {
  const lines = [
    '## Schemas',
    '',
    'JSON Schemas (draft 2020-12) for every definition file. Validate YAML against them before writing it, then run `vc check`.',
    '',
    ...KINDS.map((k) => `- [${k}.schema.json](${schemaUrl(k)}): ${schemaDescription[k]}`),
    `- [Catalog](${SCHEMA_BASE}index.json): SchemaStore-style catalog with fileMatch globs for all four`,
    '',
  ];
  return lines.join('\n');
}

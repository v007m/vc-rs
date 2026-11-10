import type { APIRoute } from 'astro';
import { fileMatch, KINDS, schemaDescription, schemaUrl, schemaText } from '~/lib/schemas';

export const GET: APIRoute = () => {
  const catalog = {
    $schema: 'https://json.schemastore.org/schema-catalog.json',
    version: 1,
    schemas: KINDS.map((kind) => ({
      name: (JSON.parse(schemaText[kind]) as { title?: string }).title ?? `Vibranium ${kind} file`,
      description: schemaDescription[kind],
      fileMatch: fileMatch[kind],
      url: schemaUrl(kind),
    })),
  };
  return new Response(`${JSON.stringify(catalog, null, 2)}\n`, {
    headers: { 'Content-Type': 'application/json; charset=utf-8' },
  });
};

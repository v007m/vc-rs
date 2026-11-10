import type { APIRoute, GetStaticPaths } from 'astro';
import { KINDS, schemaText, type Kind } from '~/lib/schemas';

export const getStaticPaths = (() => KINDS.map((kind) => ({ params: { kind } }))) satisfies GetStaticPaths;

export const GET: APIRoute = ({ params }) =>
  new Response(schemaText[params.kind as Kind], {
    headers: { 'Content-Type': 'application/schema+json; charset=utf-8' },
  });

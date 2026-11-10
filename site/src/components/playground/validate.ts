import Ajv2020, { type ErrorObject, type ValidateFunction } from 'ajv/dist/2020';
import { isMap, isPair, isScalar, isSeq, LineCounter, parseDocument, type Node } from 'yaml';
import { schemas, type Kind } from './schemas';

export interface Problem {
  from: number;
  to: number;
  line: number;
  col: number;
  path: string;
  message: string;
  severity: 'error' | 'warning';
}

const ajv = new Ajv2020({ allErrors: true, strict: false, validateFormats: false, verbose: true });
const compiled = new Map<Kind, ValidateFunction>();

function validatorFor(kind: Kind): ValidateFunction {
  let v = compiled.get(kind);
  if (!v) {
    v = ajv.compile(schemas[kind]);
    compiled.set(kind, v);
  }
  return v;
}

type Seg = string | number;

function segments(pointer: string): Seg[] {
  if (!pointer) return [];
  return pointer
    .slice(1)
    .split('/')
    .map((s) => s.replace(/~1/g, '/').replace(/~0/g, '~'))
    .map((s) => (/^\d+$/.test(s) ? Number(s) : s));
}

function displayPath(segs: Seg[]): string {
  let out = '';
  for (const s of segs) out += typeof s === 'number' ? `[${s}]` : out ? `.${s}` : s;
  return out || '(root)';
}

function findPair(node: unknown, key: string) {
  if (!isMap(node)) return undefined;
  return node.items.find((p) => isPair(p) && isScalar(p.key) && String(p.key.value) === key);
}

function nodeAt(root: unknown, segs: Seg[]): { node: Node | null; keyNode: Node | null } {
  let node = root as Node | null;
  let keyNode: Node | null = null;
  for (const s of segs) {
    if (isSeq(node) && typeof s === 'number') {
      node = (node.items[s] as Node) ?? null;
      keyNode = null;
    } else {
      const pair = findPair(node, String(s));
      if (!pair) break;
      keyNode = pair.key as Node;
      node = (pair.value as Node) ?? keyNode;
    }
  }
  return { node, keyNode };
}

function rangeOf(n: Node | null, fallbackLen: number): [number, number] {
  const r = n?.range;
  if (!r) return [0, Math.min(1, fallbackLen)];
  return [r[0], Math.max(r[0] + 1, r[1])];
}

function describeAlternatives(schema: unknown): string {
  const parts: string[] = [];
  for (const alt of (schema as Array<Record<string, unknown>>) ?? []) {
    if (alt.type) parts.push(String(alt.type));
    else if (Array.isArray(alt.enum)) parts.push(alt.enum.map((e) => JSON.stringify(e)).join(' | '));
    else if (alt.$ref) parts.push('an object');
  }
  return [...new Set(parts)].join(', or ');
}

function requiredAlternatives(schema: unknown): string[] | null {
  const alts = schema as Array<Record<string, unknown>>;
  if (!Array.isArray(alts)) return null;
  const keys: string[] = [];
  for (const a of alts) {
    if (!Array.isArray(a.required) || a.required.length !== 1 || Object.keys(a).length !== 1) return null;
    keys.push(String(a.required[0]));
  }
  return keys;
}

function collapseComposites(errors: ErrorObject[]): ErrorObject[] {
  const composites = errors
    .filter((e) => e.keyword === 'oneOf' || e.keyword === 'anyOf')
    .sort((a, b) => b.schemaPath.length - a.schemaPath.length);
  let rest = errors.slice();
  for (const c of composites) {
    if (!rest.includes(c)) continue;
    const prefix = `${c.schemaPath}/`;
    const branch = rest.filter((e) => e.schemaPath.startsWith(prefix));
    rest = rest.filter((e) => e !== c && !e.schemaPath.startsWith(prefix));
    const names = requiredAlternatives(c.schema);
    const passing = (c.params as { passingSchemas?: number[] | null }).passingSchemas;
    if (names && passing) {
      const both = passing.map((i) => `\`${names[i]}\``).join(' and ');
      rest.push({ ...c, message: `use only one of ${both}` });
    } else if (names) {
      rest.push({ ...c, message: `needs one of ${names.map((n) => `\`${n}\``).join(', ')}` });
    } else {
      const deep = branch.filter(
        (e) => e.instancePath !== c.instancePath || !['type', 'enum', 'const', 'required'].includes(e.keyword),
      );
      if (deep.length) rest.push(...deep);
      else rest.push({ ...c, message: `must be ${describeAlternatives(c.schema)}` });
    }
  }
  return rest;
}

function messageFor(e: ErrorObject): string {
  const p = e.params as Record<string, unknown>;
  switch (e.keyword) {
    case 'additionalProperties':
      return `unknown key \`${String(p.additionalProperty)}\``;
    case 'required':
      return `missing required key \`${String(p.missingProperty)}\``;
    case 'enum':
      return `must be one of: ${(p.allowedValues as unknown[]).map((v) => JSON.stringify(v)).join(', ')}`;
    case 'type':
      return `must be ${String(p.type)}`;
    default:
      return e.message ?? 'is not valid';
  }
}

function problemFor(e: ErrorObject, doc: ReturnType<typeof parseDocument>, lc: LineCounter, len: number): Problem {
  const segs = segments(e.instancePath);
  const { node, keyNode } = nodeAt(doc.contents, segs);
  let target: Node | null = keyNode ?? node;
  if (e.keyword === 'additionalProperties') {
    const pair = findPair(node, String((e.params as Record<string, unknown>).additionalProperty));
    target = (pair?.key as Node) ?? target;
  } else if (e.keyword !== 'required' && node && !isMap(node) && !isSeq(node)) {
    target = node;
  }
  const [from, to] = rangeOf(target, len);
  const pos = lc.linePos(from);
  return { from, to, line: pos.line, col: pos.col, path: displayPath(segs), message: messageFor(e), severity: 'error' };
}

export function validate(kind: Kind, text: string): Problem[] {
  const lc = new LineCounter();
  const doc = parseDocument(text, { lineCounter: lc, prettyErrors: false, uniqueKeys: true });
  const problems: Problem[] = [];
  for (const err of [...doc.errors, ...doc.warnings]) {
    const [from, to] = err.pos;
    const pos = lc.linePos(from);
    problems.push({
      from,
      to: Math.max(from + 1, to),
      line: pos.line,
      col: pos.col,
      path: 'YAML',
      message: err.message.split('\n')[0],
      severity: doc.errors.includes(err as never) ? 'error' : 'warning',
    });
  }
  if (doc.errors.length) return problems;
  const data = doc.toJS({ maxAliasCount: 100 }) ?? {};
  const validator = validatorFor(kind);
  if (validator(data)) return problems;
  const seen = new Set<string>();
  for (const e of collapseComposites(validator.errors ?? [])) {
    const p = problemFor(e, doc, lc, text.length);
    const key = `${p.path}|${p.message}`;
    if (seen.has(key)) continue;
    seen.add(key);
    problems.push(p);
  }
  return problems.sort((a, b) => a.from - b.from).map((p) => ({ ...p, to: Math.min(p.to, Math.max(text.length, 1)) }));
}

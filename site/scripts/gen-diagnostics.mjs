#!/usr/bin/env node
// Generates the diagnostics reference from the `vc explain` pages.
//
// Reads   crates/diagnostics/explain/Vxxxx.md      (wrong/right pages, the source of truth)
//         crates/diagnostics/src/code/table.rs     (severity of each code)
//         docs/design/lld.md "Diagnostic codes"    (run-time outcome and certain fix)
//         site/scripts/diagnostics-examples.json   (real `vc` output, captured by hand)
//         site/scripts/diagnostics-index.template.mdx
// Writes  site/src/content/docs/reference/diagnostics/vxxxx.mdx and index.mdx
//
// Usage: node site/scripts/gen-diagnostics.mjs
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repo = join(scriptDir, '..', '..');
const explainDir = join(repo, 'crates', 'diagnostics', 'explain');
const tableFile = join(repo, 'crates', 'diagnostics', 'src', 'code', 'table.rs');
const lldFile = join(repo, 'docs', 'design', 'lld.md');
const examplesFile = join(scriptDir, 'diagnostics-examples.json');
const indexTemplate = join(scriptDir, 'diagnostics-index.template.mdx');
const outDir = join(repo, 'site', 'src', 'content', 'docs', 'reference', 'diagnostics');
const base = '/vc-rs';

const CATEGORIES = {
  1: { range: 'V1xxx', name: 'Syntax', blurb: 'the file cannot be read as YAML of the right shape' },
  2: { range: 'V2xxx', name: 'References and types', blurb: 'an ID, var, secret or value does not resolve or has the wrong type' },
  3: { range: 'V3xxx', name: 'Lifecycle', blurb: 'profiles, skips and setup or cleanup that is legal but risky' },
  4: { range: 'V4xxx', name: 'Conventions', blurb: 'names that break the house naming rules' },
  5: { range: 'V5xxx', name: 'Run time', blurb: 'what went wrong while a test, hook or plugin ran' },
};

/** A V1–V3 code is reported by `vc check`, so `explain.rs` holds its wrong example to it. */
const checked = (code) => '123'.includes(code[1]);

/**
 * The info words that mark a block of a V1–V3 wrong example for the docs check
 * (`crates/engine/tests/docs_examples.rs`): `wrong <code>` keeps it out of the page's project
 * and makes the example report that code.
 */
const wrongTag = (code) => (checked(code) ? ` wrong ${code}` : '');

const readIf = (file) => (existsSync(file) ? readFileSync(file, 'utf8') : '');

/** Severity per code from the Rust code table: `V1001 = 1001, Error, "…";`. */
function readSeverities() {
  const map = new Map();
  for (const m of readIf(tableFile).matchAll(/^\s*V(\d{4}) = \d+, (\w+),/gm)) {
    map.set(`V${m[1]}`, m[2].toLowerCase());
  }
  return map;
}

/** Outcome and certain fix per code from the LLD table: `| V1002 | error | … | fix |`. */
function readLldRows() {
  const map = new Map();
  for (const line of readIf(lldFile).split('\n')) {
    const m = line.match(/^\| (V\d{4}) \| (\w+) \| .* \| (.*) \|$/);
    if (m) map.set(m[1], { outcome: m[2], fix: m[3].trim() });
  }
  return map;
}

/** Escapes MDX-hostile characters in prose, leaving inline code spans as they are. */
function escapeProse(line) {
  return line
    .split(/(`[^`]*`)/)
    .map((part) => (part.startsWith('`') ? part : part.replace(/[{}]/g, (c) => `\\${c}`).replace(/</g, '&lt;')))
    .join('');
}

const plain = (text) => text.replace(/`/g, '').trim();

/** First sentence of the prose, for the page description. */
function firstSentence(prose) {
  const text = plain(prose.join(' ').replace(/\s+/g, ' '));
  const m = text.match(/^(.+?[.!?])(\s|$)/);
  return m ? m[1] : text;
}

/**
 * Turns a fenced block whose first line is `# file: <path>` into a titled code block, its info
 * string kept and `tag` added after it.
 */
function convertFence(lines, start, tag) {
  const lang = (lines[start].slice(3).trim() || 'yaml') + tag;
  let end = start + 1;
  while (end < lines.length && !lines[end].startsWith('```')) end += 1;
  let body = lines.slice(start + 1, end);
  let title = '';
  const marker = body[0]?.match(/^# file: (.+)$/);
  if (marker) {
    title = ` title="${marker[1].trim()}"`;
    body = body.slice(1);
  }
  return { text: ['```' + lang + title, ...body, '```'].join('\n'), next: end + 1 };
}

/** Parses one explain page into its code, title, prose and converted body. */
function parsePage(source) {
  const lines = source.replace(/\r\n/g, '\n').split('\n');
  const head = lines[0].match(/^# (V\d{4}): (.+)$/);
  if (!head) throw new Error(`first line is not "# Vxxxx: title": ${lines[0]}`);
  const prose = [];
  const body = [];
  let i = 1;
  let inSections = false;
  let section = '';
  let paragraph = [];
  // Prose is escaped a paragraph at a time, since an inline code span can wrap a line.
  const flush = () => {
    if (paragraph.length) body.push(escapeProse(paragraph.join('\n')));
    paragraph = [];
  };
  while (i < lines.length) {
    const line = lines[i];
    if (line.startsWith('```')) {
      flush();
      const fence = convertFence(lines, i, section === 'Wrong' ? wrongTag(head[1]) : '');
      body.push(fence.text);
      i = fence.next;
      continue;
    }
    if (/^## /.test(line)) {
      inSections = true;
      section = line.slice(3).trim();
    }
    if (!inSections && line.trim()) prose.push(line);
    if (line.startsWith('#') || !line.trim()) {
      flush();
      body.push(line);
    } else {
      paragraph.push(line);
    }
    i += 1;
  }
  flush();
  return { code: head[1], title: head[2].trim(), prose, body: body.join('\n').trim() };
}

const severityBadge = (severity) =>
  severity === 'warning' ? '<Badge text="warning" variant="caution" />' : '<Badge text="error" variant="danger" />';

function factsTable(page, severity, lld) {
  const cat = CATEGORIES[page.code[1]];
  const rows = [
    ['Code', `\`${page.code}\``],
    ['Category', `${cat.range}: ${cat.name}`],
    ['Severity', severity ?? 'error'],
  ];
  if (lld && lld.outcome !== severity && page.code.startsWith('V5')) rows.push(['Test outcome', lld.outcome]);
  if (lld) rows.push(['Certain fix', lld.fix === '—' ? 'none; `vc check --fix` leaves this one to you' : escapeProse(lld.fix)]);
  return ['| | |', '| --- | --- |', ...rows.map(([k, v]) => `| ${k} | ${v} |`)].join('\n');
}

function seenInPractice(code, example) {
  if (!example) return '';
  const tag = example.output.includes(code) ? wrongTag(code) : '';
  const files = example.files.map((f) => ['```' + f.lang + tag + ` title="${f.path}"`, f.code, '```'].join('\n'));
  return [
    '## Seen in practice',
    '',
    'This is real output from `vc`, run against the files below.',
    '',
    ...files.flatMap((f) => [f, '']),
    '```console',
    `$ ${example.command}`,
    example.output,
    `$ echo $?`,
    String(example.exit),
    '```',
    '',
  ].join('\n');
}

function renderPage(page, severity, lld, example) {
  const description = firstSentence(page.prose);
  const front = [
    '---',
    `title: ${JSON.stringify(`${page.code} — ${plain(page.title)}`)}`,
    `description: ${JSON.stringify(description)}`,
    'sidebar:',
    `  label: ${JSON.stringify(`${page.code} ${plain(page.title)}`)}`,
    `  order: ${Number(page.code.slice(1))}`,
    '---',
  ].join('\n');
  return [
    front,
    '',
    "import { Badge } from '@astrojs/starlight/components';",
    '',
    `${severityBadge(severity)} <Badge text="${CATEGORIES[page.code[1]].name}" variant="note" />`,
    '',
    factsTable(page, severity, lld),
    '',
    page.body,
    '',
    seenInPractice(page.code, example),
    '## Explain it in your terminal',
    '',
    'The same page is built into the binary, so this works anywhere, even outside a project:',
    '',
    '```console',
    `$ vc explain ${page.code}`,
    '```',
    '',
    '## Related',
    '',
    `- [All diagnostic codes](${base}/reference/diagnostics/)`,
    `- [\`vc explain\`](${base}/reference/cli/explain/)`,
    `- [\`vc check\`](${base}/reference/cli/check/)`,
    `- [Errors and fixes](${base}/guides/errors-and-fixes/)`,
    '',
  ].join('\n');
}

function codeTable(pages, severities) {
  const out = [];
  for (const digit of Object.keys(CATEGORIES)) {
    const cat = CATEGORIES[digit];
    const inCat = pages.filter((p) => p.code[1] === digit);
    out.push(`### ${cat.range}: ${cat.name}`, '', `${cat.name} codes: ${cat.blurb}.`, '');
    out.push('| Code | Severity | Meaning |', '| --- | --- | --- |');
    for (const p of inCat) {
      const slug = p.code.toLowerCase();
      out.push(`| [${p.code}](${base}/reference/diagnostics/${slug}/) | ${severities.get(p.code) ?? 'error'} | ${escapeProse(p.title)} |`);
    }
    out.push('');
  }
  return out.join('\n');
}

function main() {
  const severities = readSeverities();
  const lld = readLldRows();
  const examples = existsSync(examplesFile) ? JSON.parse(readFileSync(examplesFile, 'utf8')) : {};
  const files = readdirSync(explainDir).filter((f) => /^V\d{4}\.md$/.test(f)).sort();
  mkdirSync(outDir, { recursive: true });
  const pages = files.map((f) => parsePage(readFileSync(join(explainDir, f), 'utf8')));
  for (const page of pages) {
    const text = renderPage(page, severities.get(page.code), lld.get(page.code), examples[page.code]);
    writeFileSync(join(outDir, `${page.code.toLowerCase()}.mdx`), text);
  }
  const template = readFileSync(indexTemplate, 'utf8');
  const index = template
    .replace('{/* CODE_COUNT */}', String(pages.length))
    .replace('{/* CODE_TABLE */}', codeTable(pages, severities));
  writeFileSync(join(outDir, 'index.mdx'), index);
  const missing = Object.keys(examples).filter((c) => !pages.some((p) => p.code === c));
  if (missing.length) console.warn(`examples for unknown codes: ${missing.join(', ')}`);
  console.log(`wrote ${pages.length} code pages and index.mdx to ${outDir}`);
}

main();

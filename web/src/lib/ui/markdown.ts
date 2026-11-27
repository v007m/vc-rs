/** A run of text inside a block: plain, or `code`. */
export type Span = Readonly<{ text: string; code: boolean }>;

/**
 * The Markdown the engine sends (an explain page, a hover): headings, paragraphs, bullet lists and
 * fenced code. Anything else reads as a paragraph; nothing in it ever becomes markup.
 */
export type Block =
  | Readonly<{ kind: 'heading'; level: 1 | 2 | 3; spans: readonly Span[] }>
  | Readonly<{ kind: 'paragraph'; spans: readonly Span[] }>
  | Readonly<{ kind: 'list'; items: readonly (readonly Span[])[] }>
  | Readonly<{ kind: 'code'; text: string }>;

/** `text` split at its backticks: every other run is inline code. */
export function spans(text: string): Span[] {
  return text
    .split('`')
    .map((part, i) => ({ text: part, code: i % 2 === 1 }))
    .filter((span) => span.text !== '');
}

type Lines = { readonly all: readonly string[]; at: number };

function fence(lines: Lines): Block {
  const body: string[] = [];
  lines.at += 1;
  while (lines.at < lines.all.length && !lines.all[lines.at]?.startsWith('```')) {
    body.push(lines.all[lines.at] ?? '');
    lines.at += 1;
  }
  lines.at += 1;
  return { kind: 'code', text: body.join('\n') };
}

function list(lines: Lines): Block {
  const items: Span[][] = [];
  while (/^[-*] /.test(lines.all[lines.at] ?? '')) {
    items.push(spans((lines.all[lines.at] ?? '').slice(2).trim()));
    lines.at += 1;
  }
  return { kind: 'list', items };
}

function paragraph(lines: Lines): Block {
  const text: string[] = [];
  while (lines.at < lines.all.length && !starts(lines.all[lines.at] ?? '')) {
    text.push((lines.all[lines.at] ?? '').trim());
    lines.at += 1;
  }
  return { kind: 'paragraph', spans: spans(text.join(' ')) };
}

const headingLine = /^(#{1,6}) ([^\n]*)$/;

/** Whether `line` ends a paragraph: a blank line or the start of another block. */
function starts(line: string): boolean {
  return line.trim() === '' || /^(#{1,6} |```|[-*] )/.test(line);
}

/** One line as a paragraph of its own, when no block would take it. */
function single(lines: Lines): Block {
  const text = (lines.all[lines.at] ?? '').trim();
  lines.at += 1;
  return { kind: 'paragraph', spans: spans(text) };
}

function block(lines: Lines): Block | undefined {
  const line = lines.all[lines.at] ?? '';
  const heading = headingLine.exec(line);
  if (line.trim() === '') {
    lines.at += 1;
    return undefined;
  }
  if (heading) {
    lines.at += 1;
    const level = Math.min(3, heading[1]?.length ?? 1) as 1 | 2 | 3;
    return { kind: 'heading', level, spans: spans(heading[2] ?? '') };
  }
  if (line.startsWith('```')) return fence(lines);
  if (/^[-*] /.test(line)) return list(lines);
  return paragraph(lines);
}

/** `markdown` as blocks, in order. Every line break, `\r` and U+2028/U+2029 too, ends a line. */
export function blocks(markdown: string): Block[] {
  const lines: Lines = { all: markdown.split(/\r\n|[\r\n\u2028\u2029]/), at: 0 };
  const out: Block[] = [];
  while (lines.at < lines.all.length) {
    const at = lines.at;
    const next = block(lines);
    if (next) out.push(next);
    // a block that took no line would loop forever: show the line as text and move on
    if (lines.at === at) out.push(single(lines));
  }
  return out;
}

function inline(parent: HTMLElement, runs: readonly Span[]): HTMLElement {
  for (const run of runs) {
    if (run.code) parent.appendChild(document.createElement('code')).textContent = run.text;
    else parent.appendChild(document.createTextNode(run.text));
  }
  return parent;
}

function element(item: Block): HTMLElement {
  if (item.kind === 'heading')
    return inline(document.createElement(`h${item.level + 2}`), item.spans);
  if (item.kind === 'paragraph') return inline(document.createElement('p'), item.spans);
  if (item.kind === 'code') {
    const pre = document.createElement('pre');
    pre.appendChild(document.createElement('code')).textContent = item.text;
    return pre;
  }
  const ul = document.createElement('ul');
  for (const runs of item.items) inline(ul.appendChild(document.createElement('li')), runs);
  return ul;
}

/** `markdown` as DOM built from text nodes only, for views outside Svelte (the YAML hover). */
export function markdownDom(markdown: string, className: string): HTMLElement {
  const dom = document.createElement('div');
  dom.className = className;
  for (const item of blocks(markdown)) dom.appendChild(element(item));
  return dom;
}

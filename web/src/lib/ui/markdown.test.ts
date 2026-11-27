import { render } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import { blocks, markdownDom, spans } from './markdown.ts';
import Markdown from './Markdown.svelte';

const page = [
  '# V2003 `unknown reference`',
  '',
  'A test names an id',
  'that does not exist.',
  '',
  '## Wrong',
  '```yaml',
  'use: users.crud#nope',
  '```',
  '- check the `id`',
  '- run vc check',
].join('\n');

describe('the safe Markdown renderer', () => {
  it('reads headings, paragraphs, fenced code and lists', () => {
    expect(blocks(page)).toEqual([
      {
        kind: 'heading',
        level: 1,
        spans: [
          { text: 'V2003 ', code: false },
          { text: 'unknown reference', code: true },
        ],
      },
      {
        kind: 'paragraph',
        spans: [{ text: 'A test names an id that does not exist.', code: false }],
      },
      { kind: 'heading', level: 2, spans: [{ text: 'Wrong', code: false }] },
      { kind: 'code', text: 'use: users.crud#nope' },
      {
        kind: 'list',
        items: [
          [
            { text: 'check the ', code: false },
            { text: 'id', code: true },
          ],
          [{ text: 'run vc check', code: false }],
        ],
      },
    ]);
  });

  it('keeps an unclosed backtick as code to the end of the line', () => {
    expect(spans('a `b')).toEqual([
      { text: 'a ', code: false },
      { text: 'b', code: true },
    ]);
  });

  it('builds DOM from text only, so markup in the Markdown stays text', () => {
    const dom = markdownDom('<img src=x onerror=alert(1)>\n\n```\n<script>x</script>\n```', 'md');
    expect(dom.className).toBe('md');
    expect(dom.querySelector('img, script')).toBeNull();
    expect(dom.querySelector('p')?.textContent).toBe('<img src=x onerror=alert(1)>');
    expect(dom.querySelector('pre code')?.textContent).toBe('<script>x</script>');
  });

  it('ends a heading at any line break: lone CR, U+2028 and U+2029 (D755)', () => {
    const heading = (text: string) => ({
      kind: 'heading',
      level: 1,
      spans: [{ text, code: false }],
    });
    const more = { kind: 'paragraph', spans: [{ text: 'more', code: false }] };
    expect(blocks('# Title\rmore')).toEqual([heading('Title'), more]);
    expect(blocks('# T\u2028more')).toEqual([heading('T'), more]);
    expect(blocks('# T\u2029more')).toEqual([heading('T'), more]);
    expect(blocks('# T x')).toEqual([heading('T x')]);
    expect(blocks('#\tT')).toEqual([{ kind: 'paragraph', spans: [{ text: '#\tT', code: false }] }]);
  });

  it('always ends, whatever odd characters a line holds (D755)', () => {
    const odd = [
      '#',
      ' ',
      '\r',
      '\n',
      '\u2028',
      '\u2029',
      '\u0085',
      '\v',
      '\f',
      '\t',
      '`',
      '-',
      '*',
      'x',
    ];
    let seed = 7;
    const pick = () => {
      seed = (seed * 1103515245 + 12345) % 2147483648;
      return odd[seed % odd.length] ?? 'x';
    };
    for (let round = 0; round < 2000; round += 1) {
      const text = Array.from({ length: 1 + (round % 12) }, pick).join('');
      const out = blocks(text);
      expect(out.length).toBeLessThanOrEqual(text.length + 1);
    }
  });

  it('renders the same blocks in Svelte, as text', () => {
    const { container } = render(Markdown, { text: `${page}\n\n<b>bold</b>` });
    expect(container.querySelector('h3')?.textContent).toBe('V2003 unknown reference');
    expect(container.querySelector('h4')?.textContent).toBe('Wrong');
    expect(container.querySelector('pre')?.textContent).toBe('use: users.crud#nope');
    expect(container.querySelectorAll('li')).toHaveLength(2);
    expect(container.querySelector('b')).toBeNull();
    expect(container.textContent).toContain('<b>bold</b>');
  });
});

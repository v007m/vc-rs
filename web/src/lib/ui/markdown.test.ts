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

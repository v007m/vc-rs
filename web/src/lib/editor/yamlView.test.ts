import { create } from '@bufbuild/protobuf';
import { FixSchema } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { describe, expect, it, vi } from 'vitest';
import { completions, hoverDom, lintMarks } from './yamlView.ts';

describe('the YAML view adapters', () => {
  it('turns marks into lint diagnostics, clamped to the text, each fix an action', () => {
    const fix = create(FixSchema, { title: 'use getsUser', certain: true });
    const onfix = vi.fn();
    const [lint] = lintMarks(
      [{ from: 4, to: 99, severity: 'error', code: 'V2001', message: 'unknown id', fixes: [fix] }],
      10,
      onfix,
    );
    expect(lint).toMatchObject({ from: 4, to: 10, severity: 'error', source: 'V2001' });
    expect(lint?.actions?.map((a) => a.name)).toEqual(['use getsUser']);
    lint?.actions?.[0]?.apply(undefined as never, 4, 10);
    expect(onfix).toHaveBeenCalledWith(fix, 'V2001');
  });

  it('maps suggestions to completions, inserting the label unless told otherwise', () => {
    const got = completions([
      { label: 'userId', kind: 'var', detail: 'string' },
      { label: 'eq', kind: 'matcher', insert: 'eq: ' },
      { label: 'POST', kind: 'value', doc: 'The method.' },
    ]);
    expect(got).toEqual([
      { label: 'userId', type: 'variable', detail: 'string' },
      { label: 'eq', type: 'method', apply: 'eq: ' },
      { label: 'POST', type: 'enum', info: 'The method.' },
    ]);
  });

  it('builds the hover card from the Markdown as text only, so a value cannot become markup', () => {
    const dom = hoverDom({ markdown: '## `baseUrl`\n\nThe url: <img src=x onerror=alert(1)>' });
    expect(dom.className).toBe('cm-hover');
    expect(dom.querySelector('h4 code')?.textContent).toBe('baseUrl');
    expect(dom.querySelector('p')?.textContent).toBe('The url: <img src=x onerror=alert(1)>');
    expect(dom.querySelector('img')).toBeNull();
  });
});

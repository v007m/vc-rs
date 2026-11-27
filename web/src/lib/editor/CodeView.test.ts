import { currentCompletions, startCompletion } from '@codemirror/autocomplete';
import { forEachDiagnostic, type Diagnostic as LintMark } from '@codemirror/lint';
import { create } from '@bufbuild/protobuf';
import { render } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import { available } from '#lib/api/maybe.ts';
import { FixSchema } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { findCodeView, typeAtEnd } from '#lib/testing/codeView.ts';
import { noAssist, type EditorAssist } from './assist.ts';
import CodeView from './CodeView.svelte';

const path = 'services/users/crud.test.yaml';
const fix = create(FixSchema, { title: 'Rename to getsUser', certain: true });
const mark = {
  from: 15,
  to: 22,
  severity: 'error',
  code: 'V4001',
  message: 'test IDs are camelCase',
  fixes: [fix],
} as const;

function show(assist: EditorAssist = noAssist) {
  const onedit = vi.fn();
  const onfix = vi.fn();
  const props = { text: 'name: crud\nid: GetUser\n', path, label: path, marks: [mark] };
  const shown = render(CodeView, { ...props, onedit, onfix, assist });
  return { ...shown, onedit, onfix };
}

describe('CodeView', () => {
  it('shows the text in a shadow root, so its styles meet the CSP, and reports edits', async () => {
    const { container, onedit } = show();
    const host = container.querySelector('[data-code-view]');
    expect(host?.shadowRoot?.querySelector('.cm-editor')).toBeTruthy();
    const view = await findCodeView(path);
    typeAtEnd(view, '# more');
    expect(onedit).toHaveBeenLastCalledWith('name: crud\nid: GetUser\n# more');
  });

  it('underlines the diagnostics, each certain fix a quick fix', async () => {
    const { onfix } = show();
    const view = await findCodeView(path);
    const found: LintMark[] = [];
    await vi.waitFor(() => {
      forEachDiagnostic(view.state, (d) => found.push(d));
      expect(found).toHaveLength(1);
    });
    expect(found[0]).toMatchObject({ from: 15, to: 22, severity: 'error', source: 'V4001' });
    found[0]?.actions?.[0]?.apply(view, 15, 22);
    expect(onfix).toHaveBeenCalledWith(fix, 'V4001');
  });

  it('completes from the assist seam at the cursor', async () => {
    const complete = vi.fn<EditorAssist['complete']>(() =>
      Promise.resolve(available({ from: 15, items: [{ label: 'getsUser', kind: 'id' as const }] })),
    );
    show({ ...noAssist, complete });
    const view = await findCodeView(path);
    view.dispatch({ selection: { anchor: 22 } });
    startCompletion(view);
    await vi.waitFor(() =>
      expect(currentCompletions(view.state).map((c) => c.label)).toEqual(['getsUser']),
    );
    expect(complete).toHaveBeenCalledWith(path, 'name: crud\nid: GetUser\n', 22);
  });
});

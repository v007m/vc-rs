import { EditorView } from '@codemirror/view';
import { vi } from 'vitest';

// jsdom lays nothing out; CodeMirror measures text through ranges, so give them empty boxes.
const empty = () => new DOMRect();
Range.prototype.getBoundingClientRect = empty;
Range.prototype.getClientRects = () =>
  Object.assign([], { item: () => null }) as unknown as DOMRectList;

// A shadow root in jsdom has no selection, so CodeMirror reads it the way it does in Safari.
if (typeof document.execCommand !== 'function') document.execCommand = () => false;

/** The YAML view labelled `name`, found inside its shadow root, where queries cannot see. */
export function codeView(name: string): EditorView | undefined {
  for (const host of document.querySelectorAll('[data-code-view]')) {
    const content = host.shadowRoot?.querySelector(`.cm-content[aria-label="${name}"]`);
    const view = content instanceof HTMLElement ? EditorView.findFromDOM(content) : null;
    if (view) return view;
  }
  return undefined;
}

/** Waits for the YAML view labelled `name`. */
export async function findCodeView(name: string): Promise<EditorView> {
  return vi.waitFor(() => {
    const view = codeView(name);
    if (!view) throw new Error(`no YAML view for ${name}`);
    return view;
  });
}

/** Types `text` at the end of the view's document, as a user would. */
export function typeAtEnd(view: EditorView, text: string): void {
  view.dispatch({ changes: { from: view.state.doc.length, insert: text }, userEvent: 'input' });
}

/** The selected text of the view. */
export function selected(view: EditorView): string {
  const { from, to } = view.state.selection.main;
  return view.state.sliceDoc(from, to);
}

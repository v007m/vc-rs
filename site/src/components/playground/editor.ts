import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap } from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { yaml, yamlLanguage } from '@codemirror/lang-yaml';
import { bracketMatching, HighlightStyle, indentOnInput, syntaxHighlighting } from '@codemirror/language';
import { linter, lintGutter, type Diagnostic } from '@codemirror/lint';
import { EditorState } from '@codemirror/state';
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  hoverTooltip,
  keymap,
  lineNumbers,
} from '@codemirror/view';
import { tags as t } from '@lezer/highlight';
import { stateExtensions } from 'codemirror-json-schema';
import { yamlCompletion, yamlSchemaHover } from 'codemirror-json-schema/yaml';
import { schemas, type Kind } from './schemas';
import { validate, type Problem } from './validate';

export interface EditorOptions {
  kind: Kind;
  doc: string;
  onProblems: (problems: Problem[]) => void;
  /** Called with the whole text after each edit. */
  onChange?: (doc: string) => void;
}

const highlight = HighlightStyle.define([
  { tag: [t.propertyName, t.definition(t.propertyName)], color: 'var(--pg-key)' },
  { tag: [t.string, t.special(t.string), t.content], color: 'var(--pg-string)' },
  { tag: [t.number, t.bool, t.null, t.atom], color: 'var(--pg-number)' },
  { tag: [t.comment, t.lineComment], color: 'var(--pg-comment)', fontStyle: 'italic' },
  { tag: [t.punctuation, t.separator, t.squareBracket, t.brace], color: 'var(--pg-punct)' },
  { tag: [t.keyword, t.typeName, t.labelName, t.meta], color: 'var(--pg-meta)' },
]);

const theme = EditorView.theme({
  '&': {
    backgroundColor: 'var(--pg-bg)',
    color: 'var(--pg-fg)',
    fontSize: '0.85rem',
    height: '100%',
  },
  '.cm-scroller': { fontFamily: 'var(--sl-font-mono)', lineHeight: '1.6' },
  '.cm-content': { caretColor: 'var(--sl-color-accent-high)' },
  '.cm-gutters': {
    backgroundColor: 'var(--pg-bg)',
    color: 'var(--pg-comment)',
    borderRight: '1px solid var(--sl-color-hairline)',
  },
  '.cm-activeLine, .cm-activeLineGutter': { backgroundColor: 'var(--pg-active)' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
    backgroundColor: 'var(--pg-selection) !important',
  },
  '.cm-tooltip': {
    backgroundColor: 'var(--sl-color-bg-nav)',
    color: 'var(--sl-color-text)',
    border: '1px solid var(--sl-color-gray-5)',
    borderRadius: '6px',
    maxWidth: 'min(28rem, 90vw)',
  },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    backgroundColor: 'var(--sl-color-accent)',
    color: 'var(--sl-color-white)',
  },
  '.cm-tooltip.cm-tooltip-hover, .cm-tooltip-lint': { padding: '0.4rem 0.6rem', fontSize: '0.8rem' },
  '.cm-tooltip-hover code': { fontFamily: 'var(--sl-font-mono)' },
  '.cm-tooltip-hover p': { margin: '0.25rem 0' },
  '.cm-diagnostic-error': { borderLeftColor: 'var(--pg-error)' },
});

function schemaLinter(kind: Kind, onProblems: (p: Problem[]) => void) {
  return linter(
    (view): Diagnostic[] => {
      const problems = validate(kind, view.state.doc.toString());
      onProblems(problems);
      return problems.map((p) => ({
        from: p.from,
        to: p.to,
        severity: p.severity,
        message: p.path === 'YAML' ? p.message : `${p.path}: ${p.message}`,
      }));
    },
    { delay: 250 },
  );
}

export function createEditor(parent: HTMLElement, opts: EditorOptions): EditorView {
  const state = EditorState.create({
    doc: opts.doc,
    extensions: [
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightActiveLine(),
      history(),
      drawSelection(),
      indentOnInput(),
      bracketMatching(),
      closeBrackets(),
      autocompletion(),
      lintGutter(),
      keymap.of([...closeBracketsKeymap, ...defaultKeymap, ...historyKeymap, ...completionKeymap, indentWithTab]),
      yaml(),
      syntaxHighlighting(highlight),
      theme,
      EditorView.lineWrapping,
      yamlLanguage.data.of({ autocomplete: yamlCompletion() }),
      hoverTooltip(yamlSchemaHover()),
      stateExtensions(schemas[opts.kind] as never),
      schemaLinter(opts.kind, opts.onProblems),
      EditorView.contentAttributes.of({ 'aria-label': `${opts.kind} YAML editor` }),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) opts.onChange?.(update.state.doc.toString());
      }),
    ],
  });
  return new EditorView({ state, parent });
}

export function setDoc(view: EditorView, doc: string): void {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: doc } });
}

export function jumpTo(view: EditorView, pos: number): void {
  view.dispatch({ selection: { anchor: pos }, scrollIntoView: true });
  view.focus();
}

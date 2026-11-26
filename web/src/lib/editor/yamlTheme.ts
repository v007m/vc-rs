import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';
import type { Extension } from '@codemirror/state';

/**
 * The YAML view's look, all from the design tokens, so it follows light and dark and the tint.
 * The styles are CodeMirror's own style modules: the view lives in a shadow root, where they are
 * adopted style sheets, which the page's CSP allows (D568).
 */
const theme = EditorView.theme({
  '&': {
    height: '100%',
    minHeight: '420px',
    border: '1px solid var(--l2-border)',
    borderRadius: 'var(--r-md)',
    background: 'var(--well-bg)',
    color: 'var(--ink)',
    fontSize: '12.5px',
  },
  '&.cm-focused': { outline: '2px solid var(--focus)', outlineOffset: '0' },
  '.cm-scroller': { fontFamily: 'var(--mono)', lineHeight: '20px' },
  '.cm-content': { padding: '10px 0', caretColor: 'var(--ink)' },
  '.cm-gutters': {
    border: '0',
    background: 'transparent',
    color: 'var(--ink-3)',
  },
  '.cm-activeLine, .cm-activeLineGutter': { background: 'var(--l0-hover)' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
    background: 'var(--syn-var-bg)',
  },
  '.cm-cursor': { borderLeftColor: 'var(--ink)' },
  '.cm-tooltip': {
    border: '1px solid var(--l3-border)',
    borderRadius: 'var(--r-md)',
    background: 'var(--l3-bg)',
    color: 'var(--ink)',
    boxShadow: 'var(--sh-l3)',
    fontFamily: 'var(--font)',
  },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    background: 'var(--syn-var-bg)',
    color: 'var(--ink)',
  },
  // CodeMirror draws squiggles as data: images, which the CSP blocks; a wavy underline needs none.
  '.cm-lintRange': {
    backgroundImage: 'none',
    textDecoration: 'underline wavy',
    textUnderlineOffset: '3px',
  },
  '.cm-lintRange-error': { backgroundImage: 'none', textDecorationColor: 'var(--st-fail)' },
  '.cm-lintRange-warning': {
    backgroundImage: 'none',
    textDecorationColor: 'var(--st-blocked-ink)',
  },
  '.cm-lintRange-info, .cm-lintRange-hint': {
    backgroundImage: 'none',
    textDecorationColor: 'var(--ink-3)',
  },
  '.cm-diagnostic': { padding: '6px 10px', borderLeftWidth: '4px' },
  '.cm-diagnostic-error': { borderLeftColor: 'var(--st-fail)' },
  '.cm-diagnostic-warning': { borderLeftColor: 'var(--st-blocked-ink)' },
  '.cm-diagnosticAction': {
    marginLeft: '8px',
    padding: '2px 8px',
    border: '1px solid var(--primary-border)',
    borderRadius: 'var(--r-sm)',
    background: 'var(--primary-bg)',
    color: 'var(--primary-ink)',
    font: 'inherit',
  },
  '.cm-hover': { maxWidth: '420px', padding: '8px 10px', fontSize: '12.5px' },
  '.cm-hover-title': { fontFamily: 'var(--mono)', fontWeight: '600' },
  '.cm-hover-meta': { color: 'var(--ink-2)', fontFamily: 'var(--mono)' },
  '.cm-var': { borderRadius: '3px', background: 'var(--syn-var-bg)', color: 'var(--syn-var)' },
});

const highlight = HighlightStyle.define([
  { tag: [tags.propertyName, tags.definition(tags.propertyName)], color: 'var(--syn-key)' },
  { tag: [tags.string, tags.special(tags.string), tags.content], color: 'var(--syn-string)' },
  { tag: tags.number, color: 'var(--syn-number)' },
  { tag: tags.bool, color: 'var(--syn-bool)' },
  { tag: tags.null, color: 'var(--syn-null)' },
  { tag: [tags.punctuation, tags.separator, tags.meta], color: 'var(--syn-punct)' },
  { tag: [tags.comment, tags.lineComment], color: 'var(--ink-3)', fontStyle: 'italic' },
  { tag: [tags.labelName, tags.typeName], color: 'var(--syn-bool)' },
]);

export const yamlTheme: Extension = [theme, syntaxHighlighting(highlight)];

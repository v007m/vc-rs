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
    border: '1px solid var(--st-edge-hairline)',
    borderRadius: 'var(--st-radius-md)',
    background: 'var(--st-surface-input-on-card)',
    color: 'var(--st-text-primary)',
    fontSize: '12.5px',
  },
  '&.cm-focused': { outline: '2px solid var(--st-accent)', outlineOffset: '0' },
  '.cm-scroller': { fontFamily: 'var(--st-font-mono)', lineHeight: '20px' },
  '.cm-content': { padding: '10px 0', caretColor: 'var(--st-text-primary)' },
  '.cm-gutters': {
    border: '0',
    background: 'transparent',
    color: 'var(--st-text-tertiary)',
  },
  '.cm-activeLine, .cm-activeLineGutter': { background: 'var(--st-chrome-hover)' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
    background: 'var(--vc-syn-var-bg)',
  },
  '.cm-cursor': { borderLeftColor: 'var(--st-text-primary)' },
  '.cm-tooltip': {
    border: '1px solid var(--st-edge-rim)',
    borderRadius: 'var(--st-radius-md)',
    background: 'var(--st-surface-overlay)',
    color: 'var(--st-text-primary)',
    boxShadow: 'var(--vc-shadow-overlay)',
    fontFamily: 'var(--st-font-sans)',
  },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    background: 'var(--vc-syn-var-bg)',
    color: 'var(--st-text-primary)',
  },
  // CodeMirror draws squiggles as data: images, which the CSP blocks; a wavy underline needs none.
  '.cm-lintRange': {
    backgroundImage: 'none',
    textDecoration: 'underline wavy',
    textUnderlineOffset: '3px',
  },
  '.cm-lintRange-error': { backgroundImage: 'none', textDecorationColor: 'var(--vc-fail)' },
  '.cm-lintRange-warning': {
    backgroundImage: 'none',
    textDecorationColor: 'var(--vc-blocked-ink)',
  },
  '.cm-lintRange-info, .cm-lintRange-hint': {
    backgroundImage: 'none',
    textDecorationColor: 'var(--st-text-tertiary)',
  },
  // No leading-edge bar (D506): the tone is a dot before the message.
  '.cm-diagnostic': { padding: '6px 10px', borderLeft: '0' },
  '.cm-diagnostic::before': {
    content: '""',
    display: 'inline-block',
    width: '8px',
    height: '8px',
    marginRight: '8px',
    borderRadius: '50%',
    verticalAlign: 'middle',
    background: 'var(--st-text-tertiary)',
  },
  '.cm-diagnostic-error::before': { background: 'var(--vc-fail)' },
  '.cm-diagnostic-warning::before': { background: 'var(--vc-blocked-ink)' },
  '.cm-diagnosticAction': {
    marginLeft: '8px',
    padding: '2px 8px',
    border: '0',
    borderRadius: 'var(--st-radius-capsule)',
    background: 'var(--st-accent)',
    color: 'var(--st-text-on-accent)',
    font: 'inherit',
  },
  '.cm-hover': { maxWidth: '420px', padding: '8px 10px', fontSize: '12.5px' },
  '.cm-hover-title': { fontFamily: 'var(--st-font-mono)', fontWeight: '600' },
  '.cm-hover-meta': { color: 'var(--st-text-secondary)', fontFamily: 'var(--st-font-mono)' },
  '.cm-var': {
    borderRadius: '3px',
    background: 'var(--vc-syn-var-bg)',
    color: 'var(--vc-syn-var)',
  },
});

const highlight = HighlightStyle.define([
  { tag: [tags.propertyName, tags.definition(tags.propertyName)], color: 'var(--vc-syn-key)' },
  { tag: [tags.string, tags.special(tags.string), tags.content], color: 'var(--vc-syn-string)' },
  { tag: tags.number, color: 'var(--vc-syn-number)' },
  { tag: tags.bool, color: 'var(--vc-syn-bool)' },
  { tag: tags.null, color: 'var(--vc-syn-null)' },
  { tag: [tags.punctuation, tags.separator, tags.meta], color: 'var(--vc-syn-punct)' },
  { tag: [tags.comment, tags.lineComment], color: 'var(--st-text-tertiary)', fontStyle: 'italic' },
  { tag: [tags.labelName, tags.typeName], color: 'var(--vc-syn-bool)' },
]);

export const yamlTheme: Extension = [theme, syntaxHighlighting(highlight)];

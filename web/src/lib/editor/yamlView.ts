import {
  autocompletion,
  completionKeymap,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { yaml } from '@codemirror/lang-yaml';
import { indentOnInput } from '@codemirror/language';
import { lintKeymap, type Diagnostic as LintMark } from '@codemirror/lint';
import type { Extension } from '@codemirror/state';
import {
  Decoration,
  EditorView,
  hoverTooltip,
  keymap,
  lineNumbers,
  MatchDecorator,
  ViewPlugin,
  type DecorationSet,
  type Tooltip,
  type ViewUpdate,
} from '@codemirror/view';
import type { Fix } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import type { EditorAssist, HoverCard, Suggestion } from './assist.ts';
import type { Mark } from './marks.ts';
import { yamlTheme } from './yamlTheme.ts';

/** What the YAML view reports to its screen; read on each use, so they may change. */
export type ViewHooks = Readonly<{
  label: () => string;
  path: () => string;
  assist: () => EditorAssist;
  onedit: (text: string) => void;
}>;

const kinds: Record<Suggestion['kind'], string> = {
  id: 'function',
  var: 'variable',
  matcher: 'method',
  pipe: 'method',
  key: 'property',
};

/** The marks as CodeMirror's lint diagnostics, each certain fix a quick-fix action. */
export function lintMarks(
  marks: readonly Mark[],
  size: number,
  onfix: (fix: Fix) => void,
): LintMark[] {
  return marks.map((mark) => ({
    from: Math.min(mark.from, size),
    to: Math.min(mark.to, size),
    severity: mark.severity,
    source: mark.code,
    message: mark.message,
    actions: mark.fixes.map((fix) => ({ name: fix.title, apply: () => onfix(fix) })),
  }));
}

/** The suggestions as CodeMirror completions. */
export function completions(items: readonly Suggestion[]): Completion[] {
  return items.map(({ label, kind, detail, insert }) => ({
    label,
    type: kinds[kind],
    ...(detail === undefined ? {} : { detail }),
    ...(insert === undefined ? {} : { apply: insert }),
  }));
}

/** The hover card's DOM, from text nodes only, so no value from a file can become markup. */
export function hoverDom(card: HoverCard): HTMLElement {
  const dom = document.createElement('div');
  dom.className = 'cm-hover';
  const line = (className: string, text: string) => {
    const el = dom.appendChild(document.createElement('div'));
    el.className = className;
    el.textContent = text;
  };
  line('cm-hover-title', card.type ? `${card.title}: ${card.type}` : card.title);
  if (card.value !== undefined) line('cm-hover-meta', `= ${card.value}`);
  if (card.docs) line('cm-hover-docs', card.docs);
  return dom;
}

function completer(hooks: ViewHooks) {
  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    const word = context.matchBefore(/[\w.#${-]*/);
    if (!context.explicit && (!word || word.from === word.to)) return null;
    const text = context.state.doc.toString();
    const got = await hooks.assist().complete(hooks.path(), text, context.pos);
    return got.available ? { from: got.value.from, options: completions(got.value.items) } : null;
  };
}

function hover(hooks: ViewHooks) {
  return hoverTooltip(async (view, pos): Promise<Tooltip | null> => {
    const got = await hooks.assist().hover(hooks.path(), view.state.doc.toString(), pos);
    if (!got.available || !got.value) return null;
    const card = got.value;
    const word = view.state.wordAt(pos);
    return {
      pos: word?.from ?? pos,
      end: word?.to ?? pos,
      create: () => ({ dom: hoverDom(card) }),
    };
  });
}

/** `${…}` templates get the var colour, as in the read views. */
const templates = new MatchDecorator({
  regexp: /\$\{[^}\n]*\}/g,
  decoration: Decoration.mark({ class: 'cm-var' }),
});

const templateMarks = ViewPlugin.define(
  (view) => ({
    decorations: templates.createDeco(view),
    update(update: ViewUpdate) {
      this.decorations = templates.updateDeco(update, this.decorations as DecorationSet);
    },
  }),
  { decorations: (plugin) => plugin.decorations },
);

/** The YAML view's extensions: highlighting, history, completion, hover and the theme. */
export function yamlExtensions(hooks: ViewHooks): Extension[] {
  return [
    lineNumbers(),
    history(),
    indentOnInput(),
    yaml(),
    templateMarks,
    autocompletion({ override: [completer(hooks)] }),
    hover(hooks),
    keymap.of([...defaultKeymap, ...historyKeymap, ...completionKeymap, ...lintKeymap]),
    EditorView.contentAttributes.of({ 'aria-label': hooks.label() }),
    EditorView.updateListener.of((update) => {
      if (update.docChanged) hooks.onedit(update.state.doc.toString());
    }),
    yamlTheme,
  ];
}

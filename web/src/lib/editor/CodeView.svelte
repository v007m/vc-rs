<script lang="ts">
  import { setDiagnostics } from '@codemirror/lint';
  import { EditorState } from '@codemirror/state';
  import { EditorView } from '@codemirror/view';
  import { untrack } from 'svelte';
  import type { Fix } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
  import { noAssist, type EditorAssist } from './assist.ts';
  import type { Mark } from './marks.ts';
  import { lintMarks, yamlExtensions } from './yamlView.ts';

  /**
   * The YAML view (CodeMirror 6, D474): highlighting, completion and hover through `assist`, and
   * the file's diagnostics as squiggles with their certain fixes. It lives in a shadow root, so
   * CodeMirror's styles are adopted style sheets and the page's CSP holds (D568).
   */
  let {
    text,
    path,
    label,
    marks,
    onedit,
    onfix,
    assist = noAssist,
  }: {
    text: string;
    path: string;
    label: string;
    marks: readonly Mark[];
    onedit: (text: string) => void;
    /** A quick fix taken from a squiggle, with its diagnostic's code. */
    onfix: (fix: Fix, code: string) => void;
    assist?: EditorAssist;
  } = $props();

  let view: EditorView | undefined = $state.raw();

  /** Builds the view once; nothing it reads re-runs it, since the view owns its text from here. */
  function mount(host: HTMLElement): () => void {
    return untrack(() => build(host));
  }

  function build(host: HTMLElement): () => void {
    const root = host.shadowRoot ?? host.attachShadow({ mode: 'open' });
    const parent = root.appendChild(document.createElement('div'));
    parent.style.height = '100%';
    const hooks = {
      label: () => label,
      path: () => path,
      assist: () => assist,
      onedit: (next: string) => onedit(next),
    };
    const state = EditorState.create({ doc: text, extensions: yamlExtensions(hooks) });
    const made = new EditorView({ state, parent, root });
    view = made;
    return () => {
      made.destroy();
      parent.remove();
      view = undefined;
    };
  }

  // A revert, a fix or a reload changes the text from outside: the view takes it whole.
  $effect(() => {
    const doc = view?.state.doc.toString();
    if (view && doc !== text)
      view.dispatch({ changes: { from: 0, to: doc?.length ?? 0, insert: text } });
  });

  $effect(() => {
    if (!view) return;
    const size = view.state.doc.length;
    view.dispatch(setDiagnostics(view.state, lintMarks(marks, size, onfix)));
  });

  /** Selects `from..to`, scrolls it into view and focuses the view, as a problem's Go does. */
  export function reveal(from: number, to: number): void {
    if (!view) return;
    const size = view.state.doc.length;
    view.dispatch({
      selection: { anchor: Math.min(from, size), head: Math.min(to, size) },
      scrollIntoView: true,
    });
    view.focus();
  }
</script>

<div class="code" data-code-view {@attach mount}></div>

<style>
  .code {
    display: block;
    min-height: 420px;
    min-width: 0;
  }
</style>

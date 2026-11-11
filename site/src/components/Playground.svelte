<script lang="ts">
  import { onMount } from 'svelte';
  import type { EditorView } from '@codemirror/view';
  import type { Kind } from './playground/schemas';
  import type { Problem } from './playground/validate';

  interface Props {
    kind: Kind;
    code: string;
    height?: number;
    title?: string;
    /** The text to start from when it differs from `code`, such as an earlier edit; Reset goes back to `code`. */
    value?: string;
    /** Called with the whole text after each edit. */
    onchange?: (text: string) => void;
  }

  let { kind, code, height = 320, title, value, onchange }: Props = $props();

  function dedent(src: string): string {
    const lines = src.replace(/^\s*\n/, '').replace(/\s+$/, '').split('\n');
    const indents = lines.filter((l) => l.trim()).map((l) => l.match(/^ */)![0].length);
    const cut = indents.length ? Math.min(...indents) : 0;
    return lines.map((l) => l.slice(cut)).join('\n') + '\n';
  }

  const initial = dedent(code ?? '');
  const start = value ?? initial;
  let host: HTMLDivElement;
  let view: EditorView | undefined;
  let api: typeof import('./playground/editor') | undefined;
  let problems: Problem[] = $state([]);
  let ready = $state(false);
  let failed = $state(false);
  let copied = $state(false);

  const fileName = $derived(
    title ?? { test: 'example.test.yaml', api: 'example.api.yaml', config: 'vibranium.config.yaml', load: 'example.load.yaml' }[kind],
  );

  onMount(() => {
    let destroyed = false;
    import('./playground/editor')
      .then((mod) => {
        if (destroyed) return;
        api = mod;
        view = mod.createEditor(host, { kind, doc: start, onProblems: (p) => (problems = p), onChange: onchange });
        ready = true;
      })
      .catch(() => (failed = true));
    return () => {
      destroyed = true;
      view?.destroy();
    };
  });

  function reset() {
    if (view && api) api.setDoc(view, initial);
  }

  async function copy() {
    const text = view ? view.state.doc.toString() : start;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      copied = false;
    }
  }

  function jump(p: Problem) {
    if (view && api) api.jumpTo(view, p.from);
  }
</script>

<div class="pg not-content" data-kind={kind}>
  <div class="pg-bar">
    <span class="pg-file"><span class="pg-badge">{kind}</span>{fileName}</span>
    <span class="pg-actions">
      <button type="button" onclick={reset} disabled={!ready}>Reset</button>
      <button type="button" onclick={copy}>{copied ? 'Copied' : 'Copy'}</button>
    </span>
  </div>
  <div class="pg-editor" style:height="{height}px" bind:this={host}>
    {#if !ready}
      <pre class="pg-fallback">{start}</pre>
    {/if}
  </div>
  <div class="pg-status" role="status" aria-live="polite">
    {#if failed}
      <p class="pg-muted">The editor could not load. The YAML above is still selectable.</p>
    {:else if !ready}
      <p class="pg-muted">Loading the editor…</p>
    {:else if problems.length === 0}
      <p class="pg-ok">✓ Valid against the {kind} schema. Try a typo, or start typing a key for completions.</p>
    {:else}
      <p class="pg-count">{problems.length} problem{problems.length === 1 ? '' : 's'}</p>
      <ul>
        {#each problems as p, i (i)}
          <li>
            <button type="button" class="pg-problem" onclick={() => jump(p)}>
              <span class="pg-loc">{p.line}:{p.col}</span>
              <span class="pg-sev" class:warn={p.severity === 'warning'}>{p.severity}</span>
              {#if p.path !== 'YAML'}<code>{p.path}</code>{/if}
              <span>{p.message}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .pg {
    border: 1px solid var(--sl-color-gray-5);
    border-radius: 0.5rem;
    overflow: hidden;
    margin-block: 1.5rem;
    background: var(--pg-bg);
    max-width: 100%;
  }
  .pg-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.4rem 0.6rem;
    border-bottom: 1px solid var(--sl-color-gray-5);
    background: var(--sl-color-gray-6);
    font-size: 0.8rem;
    flex-wrap: wrap;
  }
  .pg-file {
    font-family: var(--sl-font-mono);
    color: var(--sl-color-gray-2);
    display: inline-flex;
    gap: 0.5rem;
    align-items: center;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .pg-badge {
    background: var(--sl-color-accent-low);
    color: var(--sl-color-accent-high);
    border-radius: 0.25rem;
    padding: 0 0.4rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .pg-actions {
    display: inline-flex;
    gap: 0.4rem;
  }
  .pg-actions button {
    font: inherit;
    font-size: 0.8rem;
    cursor: pointer;
    border: 1px solid var(--sl-color-gray-4);
    background: var(--sl-color-bg);
    color: var(--sl-color-text);
    border-radius: 0.3rem;
    padding: 0.2rem 0.65rem;
    min-height: 2rem;
  }
  .pg-actions button:hover:not(:disabled) {
    border-color: var(--sl-color-accent);
    color: var(--sl-color-accent-high);
  }
  .pg-actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .pg-editor {
    overflow: hidden;
    min-height: 8rem;
  }
  .pg-editor :global(.cm-editor) {
    height: 100%;
  }
  .pg-editor :global(.cm-editor.cm-focused) {
    outline: 2px solid var(--sl-color-accent);
    outline-offset: -2px;
  }
  .pg-fallback {
    margin: 0;
    padding: 0.5rem 1rem;
    font-family: var(--sl-font-mono);
    font-size: 0.85rem;
    line-height: 1.6;
    color: var(--pg-fg);
    white-space: pre-wrap;
    height: 100%;
    overflow: auto;
  }
  .pg-status {
    border-top: 1px solid var(--sl-color-gray-5);
    padding: 0.5rem 0.75rem;
    font-size: 0.8rem;
    max-height: 12rem;
    overflow: auto;
    background: var(--sl-color-gray-7, var(--sl-color-bg));
  }
  .pg-status p {
    margin: 0;
  }
  .pg-ok {
    color: var(--sl-color-green-high, var(--sl-color-text));
  }
  .pg-muted {
    color: var(--sl-color-gray-3);
  }
  .pg-count {
    font-weight: 600;
    color: var(--pg-error);
    margin-bottom: 0.25rem !important;
  }
  .pg-status ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .pg-problem {
    all: unset;
    box-sizing: border-box;
    cursor: pointer;
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem 0.5rem;
    align-items: baseline;
    width: 100%;
    padding: 0.2rem 0.25rem;
    border-radius: 0.25rem;
    color: var(--sl-color-text);
  }
  .pg-problem:hover,
  .pg-problem:focus-visible {
    background: var(--pg-active);
  }
  .pg-loc {
    font-family: var(--sl-font-mono);
    color: var(--sl-color-gray-3);
    min-width: 3.5em;
  }
  .pg-sev {
    color: var(--pg-error);
    font-weight: 600;
  }
  .pg-sev.warn {
    color: var(--sl-color-orange-high, orange);
  }
  .pg-problem code {
    font-family: var(--sl-font-mono);
    font-size: 0.75rem;
    background: var(--sl-color-gray-6);
    border-radius: 0.2rem;
    padding: 0 0.3rem;
    overflow-wrap: anywhere;
  }
</style>

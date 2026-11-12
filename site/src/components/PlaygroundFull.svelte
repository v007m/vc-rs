<script lang="ts">
  import Playground from './Playground.svelte';
  import { examples } from './playground/examples';

  import { pieces } from './playground/ansi';

  let current = $state(examples[0].id);
  const active = $derived(examples.find((e) => e.id === current) ?? examples[0]);

  /** Every file of the project as edited so far: switching tabs keeps the edits. */
  const files: Record<string, string> = $state(Object.fromEntries(examples.map((e) => [e.file, e.code])));
  let output = $state('');
  let exit: number | undefined = $state();
  let busy = $state(false);
  let runner: typeof import('./playground/runner') | undefined;
  const printed = $derived(pieces(output));

  /** `vc run` or `vc check` on the edited files, in the page; the runner loads on first use. */
  async function play(command: 'run' | 'check') {
    busy = true;
    exit = undefined;
    output = `$ vc ${command}\n`;
    try {
      runner ??= await import('./playground/runner');
      exit = await runner.play($state.snapshot(files), command, (_, text) => (output += text));
    } catch (error) {
      if (runner && error instanceof runner.Stopped) output += '\nStopped.\n';
      else output += `The runner could not start: ${error instanceof Error ? error.message : String(error)}\n`;
    } finally {
      busy = false;
    }
  }

  /** Ends the run; its `play` above then prints that it stopped and clears `busy`. */
  function stop() {
    runner?.stop();
  }

  function inlineCode(text: string): string {
    const escaped = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    return escaped.replace(/`([^`]+)`/g, '<code>$1</code>');
  }
</script>

<div class="pgf not-content">
  <div class="pgf-tabs" role="tablist" aria-label="Starter examples">
    {#each examples as ex (ex.id)}
      <button
        type="button"
        role="tab"
        aria-selected={ex.id === current}
        class:active={ex.id === current}
        onclick={() => (current = ex.id)}
      >
        <span class="pgf-kind">{ex.kind}</span>{ex.label}
      </button>
    {/each}
  </div>
  <p class="pgf-blurb">{@html inlineCode(active.blurb)}</p>
  {#key active.id}
    <Playground
      kind={active.kind}
      code={active.code}
      value={files[active.file]}
      title={active.file}
      height={460}
      onchange={(text) => (files[active.file] = text)}
    />
  {/key}
  <div class="pgf-run">
    <button type="button" class="pgf-primary" onclick={() => play('run')} disabled={busy}>Run</button>
    <button type="button" onclick={() => play('check')} disabled={busy}>Check</button>
    {#if busy}
      <button type="button" onclick={stop}>Stop</button>
    {/if}
    <span class="pgf-hint">
      Runs all nine files with the real <code>vc</code>, in your browser, against the demo service.
    </span>
  </div>
  {#if output}
    <div class="pgf-console" data-exit={exit}>
      <div class="pgf-console-bar">
        <span>Output</span>
        {#if busy}
          <span>Running…</span>
        {:else if exit !== undefined}
          <span class:pgf-failed={exit !== 0}>exit {exit}</span>
        {/if}
      </div>
      <pre role="log" aria-live="polite" aria-label="vc output">{#each printed as piece, i (i)}<span
            class={piece.style}>{piece.text}</span
          >{/each}</pre>
    </div>
  {/if}
</div>

<style>
  .pgf-tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .pgf-tabs button {
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    border: 1px solid var(--sl-color-gray-5);
    background: var(--sl-color-bg);
    color: var(--sl-color-gray-2);
    border-radius: 999px;
    padding: 0.3rem 0.8rem;
    min-height: 2.25rem;
  }
  .pgf-tabs button:hover {
    border-color: var(--sl-color-accent);
  }
  .pgf-tabs button.active {
    background: var(--sl-color-accent-low);
    border-color: var(--sl-color-accent);
    color: var(--sl-color-accent-high);
  }
  .pgf-kind {
    font-family: var(--sl-font-mono);
    font-size: 0.65rem;
    text-transform: uppercase;
    opacity: 0.75;
  }
  .pgf-blurb :global(code) {
    font-family: var(--sl-font-mono);
    font-size: 0.85em;
    background: var(--sl-color-gray-6);
    padding: 0.05em 0.3em;
    border-radius: 0.25em;
  }
  .pgf-blurb {
    margin: 1rem 0 0;
    color: var(--sl-color-gray-2);
  }
  .pgf-run {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    margin-top: -0.75rem;
  }
  .pgf-run button {
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
    border: 1px solid var(--sl-color-gray-4);
    background: var(--sl-color-bg);
    color: var(--sl-color-text);
    border-radius: 0.3rem;
    padding: 0.3rem 0.9rem;
    min-height: 2.25rem;
  }
  .pgf-run button:hover:not(:disabled) {
    border-color: var(--sl-color-accent);
  }
  .pgf-run button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .pgf-run .pgf-primary {
    background: var(--sl-color-accent);
    border-color: var(--sl-color-accent);
    color: var(--sl-color-white);
    font-weight: 600;
  }
  .pgf-hint {
    font-size: 0.8rem;
    color: var(--sl-color-gray-3);
  }
  .pgf-hint code {
    font-family: var(--sl-font-mono);
  }
  .pgf-console {
    margin-top: 1rem;
    border: 1px solid var(--sl-color-gray-5);
    border-radius: 0.5rem;
    overflow: hidden;
    background: var(--pg-bg);
  }
  .pgf-console-bar {
    display: flex;
    justify-content: space-between;
    padding: 0.4rem 0.6rem;
    font-size: 0.8rem;
    background: var(--sl-color-gray-6);
    border-bottom: 1px solid var(--sl-color-gray-5);
    color: var(--sl-color-gray-2);
  }
  .pgf-failed {
    color: var(--pg-error);
    font-weight: 600;
  }
  .pgf-console pre {
    margin: 0;
    padding: 0.6rem 0.8rem;
    max-height: 32rem;
    overflow: auto;
    font-family: var(--sl-font-mono);
    font-size: 0.8rem;
    line-height: 1.5;
    color: var(--pg-fg);
    background: var(--pg-bg);
    border: 0;
  }
  .pgf-console .b {
    font-weight: 700;
  }
  .pgf-console .d,
  .pgf-console .c90 {
    color: var(--pg-comment);
  }
  .pgf-console .c31,
  .pgf-console .c91 {
    color: var(--pg-error);
  }
  .pgf-console .c32,
  .pgf-console .c92 {
    color: var(--pg-string);
  }
  .pgf-console .c33,
  .pgf-console .c93 {
    color: var(--pg-number);
  }
  .pgf-console .c34,
  .pgf-console .c36,
  .pgf-console .c94,
  .pgf-console .c96 {
    color: var(--pg-meta);
  }
  .pgf-console .c35,
  .pgf-console .c95 {
    color: var(--pg-key);
  }
</style>

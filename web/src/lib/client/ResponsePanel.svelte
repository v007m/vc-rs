<script lang="ts">
  import CheckList from '#lib/results/CheckList.svelte';
  import Waterfall from '#lib/results/Waterfall.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import Tabs from '#lib/ui/Tabs.svelte';
  import type { AddedCheck } from './apiForm.ts';
  import BodyView from './BodyView.svelte';
  import type { CallOutput } from './port.ts';
  import { headline, parseResponse } from './response.ts';
  import { cookies, size, statusClass } from './wire.ts';

  /**
   * The response card (ui.md §4, D560): a status line, then the body, headers, cookies, timing,
   * checks, saved values and the call's own text. A body value can add a check (D562).
   */
  let {
    output,
    sending = false,
    oncheck,
    onsave,
  }: {
    output: CallOutput | undefined;
    sending?: boolean;
    oncheck?: (check: AddedCheck) => string | undefined;
    /** Saves the call as a test; absent where a call cannot be saved. */
    onsave?: () => void;
  } = $props();

  let tab = $state('body');
  const response = $derived(parseResponse(output?.responseJson));
  const timing = $derived(output?.exchange?.timing);
  const jar = $derived(response ? cookies(response.headers) : []);
  const failed = $derived(output?.checks.filter((c) => !c.passed).length ?? 0);
  const tabs = $derived([
    { id: 'body', label: 'Body' },
    { id: 'headers', label: 'Headers' },
    { id: 'cookies', label: 'Cookies' },
    ...(timing ? [{ id: 'timing', label: 'Timing' }] : []),
    { id: 'checks', label: 'Checks', ...checkCount() },
    { id: 'saved', label: 'Saved' },
    { id: 'output', label: 'Output' },
  ]);
  /** The tabs that show what the call itself reported, with or without a response. */
  const reported = new Set(['checks', 'saved', 'output']);

  function checkCount(): { count?: string; bad?: boolean } {
    const all = output?.checks.length ?? 0;
    return all ? { count: `${all - failed}/${all}`, bad: failed > 0 } : {};
  }

  /** The headline after its status code, which the chip shows: `Created · 9 ms · …`. */
  function rest(text: string, status: number | undefined): string {
    const line = headline(text);
    return status !== undefined && line.startsWith(`${status} `)
      ? line.slice(String(status).length + 1)
      : line;
  }
</script>

<section class="st-card st-card--glass" aria-label="Response">
  {#if !output}
    <div class="resp-empty vc-grow">
      <EmptyState
        icon="send"
        title={sending ? 'Sending…' : 'No response yet'}
        caption={sending ? undefined : 'Send the request to see the response (⌘↵).'}
      />
    </div>
  {:else}
    <div class="card-head">
      <p class="resp-meta vc-grow">
        {#if response}<span class="hs" data-class={statusClass(response.status)}
            >{response.status}</span
          >{/if}
        <span>{rest(output.text, response?.status)}</span>
        {#if output.wire}
          <span><b>{size(output.wire.size)}</b></span>
          {#if output.wire.protocol}<span class="vc-mono">{output.wire.protocol}</span>{/if}
        {/if}
        {#if output.checks.length}
          <span class="checks" class:is-fail={failed > 0}
            >{failed ? `${failed} failed` : 'checks pass'}</span
          >
        {/if}
      </p>
      {#if onsave}
        <button type="button" class="st-btn st-btn--sm" disabled={!response} onclick={onsave}
          >Save as test</button
        >
      {/if}
    </div>
    <Tabs
      label="Response"
      {tabs}
      bind:selected={() => (tabs.some((t) => t.id === tab) ? tab : 'body'), (id) => (tab = id)}
    >
      {#snippet panel(id)}
        {#if !response && !reported.has(id)}
          <p class="st-secondary">No response came back; the Output tab says why.</p>
        {:else if id === 'body' && response}
          <BodyView {response} {oncheck} />
        {:else if id === 'headers' && response}
          {@render pairs('Response headers', ['Header', 'Value'], response.headers)}
        {:else if id === 'cookies'}
          {#if jar.length}
            {@render pairs(
              'Cookies',
              ['Cookie', 'Value', 'Domain', 'Path', 'Expires', 'Flags'],
              jar.map((c) => [c.name, c.value, c.domain, c.path, c.expires, c.flags]),
            )}
          {:else}
            <p class="st-secondary">The response set no cookies.</p>
          {/if}
        {:else if id === 'timing' && timing}
          <Waterfall {timing} />
        {:else if id === 'checks'}
          {#if output.checks.length}
            <CheckList checks={output.checks} />
          {:else}
            <p class="st-secondary">
              No checks ran. Add one on the Expect tab, or click a value in the body.
            </p>
          {/if}
        {:else if id === 'saved'}
          {#if output.saved.length}
            {@render pairs(
              'Saved values',
              ['Name', 'From', 'Value'],
              output.saved.map((s) => [s.name, s.from, s.value]),
            )}
          {:else}
            <p class="st-secondary">The api saves nothing from its response.</p>
          {/if}
        {:else}
          <pre class="out vc-mono">{output.text}</pre>
          {#each output.notices as notice, i (i)}
            <p class="st-secondary">{notice}</p>
          {/each}
        {/if}
      {/snippet}
    </Tabs>
  {/if}
</section>

{#snippet pairs(label: string, head: readonly string[], rows: readonly (readonly string[])[])}
  <table class="client-table" aria-label={label}>
    <thead>
      <tr>
        {#each head as name (name)}<th scope="col">{name}</th>{/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as [first, ...more], i (i)}
        <tr>
          <th scope="row" class="vc-mono">{first}</th>
          {#each more as cell, j (j)}<td class="vc-mono">{cell}</td>{/each}
        </tr>
      {/each}
    </tbody>
  </table>
{/snippet}

<style>
  /* a status word in the text line, not a badge: badges never sit in a control row (D611) */
  .checks {
    color: var(--vc-pass-ink);
    font-weight: var(--st-weight-medium);
  }
  .checks.is-fail {
    color: var(--vc-fail-ink);
  }
  .out {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>

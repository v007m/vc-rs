<script lang="ts">
  import { tick } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import Icon from '#lib/shell/Icon.svelte';
  import type { AddedCheck } from './apiForm.ts';
  import { jsonRows, jsonType, type BodyFilter, type JsonRow } from './json.ts';

  /**
   * A response body as a tree (D562): objects and lists fold, a filter dims what it does not
   * match, and with `oncheck` a value is a button that adds a check on its path to Expect.
   */
  let {
    body,
    filter,
    oncheck,
  }: {
    body: unknown;
    filter?: BodyFilter;
    /** Adds the check; gives back why it cannot be added, when it cannot. */
    oncheck?: (check: AddedCheck) => string | undefined;
  } = $props();

  const collapsed = new SvelteSet<string>();
  const checked = new SvelteSet<string>();
  let picked: JsonRow | undefined = $state();
  let problem: string | undefined = $state();
  let list: HTMLUListElement | undefined = $state();
  const rows = $derived(jsonRows(body, collapsed));
  const hits = $derived(filter && 'hits' in filter ? filter : undefined);

  function toggle(path: string): void {
    if (collapsed.has(path)) collapsed.delete(path);
    else collapsed.add(path);
  }

  function pick(row: JsonRow): void {
    picked = picked?.path === row.path ? undefined : row;
    problem = undefined;
  }

  /** Closes the picker and puts focus back on the value it was opened from. */
  async function close(): Promise<void> {
    const path = picked?.path;
    picked = undefined;
    await tick();
    if (path === undefined) return;
    list?.querySelector<HTMLElement>(`button.jv[data-path="${CSS.escape(path)}"]`)?.focus();
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.key !== 'Escape' || !picked) return;
    event.stopPropagation();
    void close();
  }

  function add(op: AddedCheck['op']): void {
    if (!picked || !oncheck) return;
    const value = op === 'exists' ? 'true' : JSON.stringify(picked.value);
    problem = oncheck({ kind: 'body', path: picked.path, op, value });
    if (problem !== undefined) return;
    checked.add(picked.path);
    void close();
  }

  function shade(row: JsonRow): string | undefined {
    if (!hits || row.kind === 'close') return undefined;
    if (hits.exact.has(row.path)) return 'hit';
    return hits.hits.has(row.path) ? undefined : 'dim';
  }

  function literal(value: unknown): string {
    return JSON.stringify(value) ?? String(value);
  }
</script>

{#snippet keyOf(row: JsonRow)}
  {#if row.key !== undefined}<span class="jk"
      >{typeof row.key === 'number' ? row.key : `"${row.key}"`}</span
    ><span class="jp">: </span>{/if}
{/snippet}

<ul class="json" aria-label="Response body" bind:this={list}>
  {#each rows as row, i (`${row.path}:${row.kind}:${i}`)}
    <li data-shade={shade(row)} style:--depth={row.depth}>
      {#if row.kind === 'open'}
        <button
          type="button"
          class="tog"
          aria-expanded={!row.collapsed}
          aria-label="{row.collapsed ? 'Expand' : 'Collapse'} {row.path}"
          onclick={() => toggle(row.path)}><Icon name="chevron" small /></button
        >{@render keyOf(row)}<span class="jp">{row.bracket}</span>
        {#if row.collapsed}<span class="jsum"
            >{row.size} {row.bracket === '[' ? 'items' : 'keys'}</span
          ><span class="jp">{row.bracket === '[' ? ']' : '}'}</span>{/if}
      {:else if row.kind === 'close'}
        <span class="jp">{row.bracket === '[' ? ']' : '}'}</span>
      {:else}
        {@render keyOf(row)}
        {#if oncheck}
          <button
            type="button"
            class="jv"
            data-t={typeof row.value === 'object' ? 'null' : typeof row.value}
            aria-expanded={picked?.path === row.path}
            aria-label="{row.path} = {literal(row.value)}{checked.has(row.path)
              ? ', checked'
              : ''}. Add a check"
            data-path={row.path}
            {onkeydown}
            onclick={() => pick(row)}>{literal(row.value)}</button
          >{#if checked.has(row.path)}<span class="sr"> (checked)</span><span
              class="tick"
              aria-hidden="true">✓</span
            >{/if}
        {:else}
          <span class="jv" data-t={typeof row.value === 'object' ? 'null' : typeof row.value}
            >{literal(row.value)}</span
          >
        {/if}
      {/if}
      {#if picked?.path === row.path && row.kind === 'leaf'}
        {@render picker(row)}
      {/if}
    </li>
  {/each}
</ul>

{#snippet picker(row: JsonRow)}
  <div class="pick l3" role="group" aria-label="Add a check on {row.path}">
    <p class="pick-path mono">{row.path} = <b>{literal(row.value)}</b> · {jsonType(row.value)}</p>
    <div class="pick-ops">
      <button type="button" class="btn btn-sm" {onkeydown} onclick={() => add('equals')}
        >Equals</button
      >
      <button type="button" class="btn btn-sm" {onkeydown} onclick={() => add('exists')}
        >Exists</button
      >
      {#if typeof row.value === 'string'}
        <button type="button" class="btn btn-sm" {onkeydown} onclick={() => add('contains')}
          >Contains</button
        >
      {/if}
      <button type="button" class="btn btn-sm btn-ghost" {onkeydown} onclick={() => void close()}
        >Cancel</button
      >
    </div>
    {#if problem}<p class="pick-problem" role="alert">{problem}</p>{/if}
  </div>
{/snippet}

<style>
  .json {
    margin: 0;
    padding: var(--sp-3) var(--sp-4);
    list-style: none;
    font-family: var(--mono);
    font-size: var(--fs-2);
    line-height: 1.75;
    min-width: 0;
  }
  li {
    padding-left: calc(var(--depth) * 18px + 18px);
    white-space: nowrap;
  }
  li[data-shade='dim'] {
    opacity: 0.35;
  }
  li[data-shade='hit'] > .jk {
    border-radius: 3px;
    background: var(--accent-soft);
  }
  .tog {
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    margin: 0 2px 0 -18px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--ink-3);
    vertical-align: -3px;
  }
  .tog:hover {
    color: var(--ink);
    background: var(--row-hover);
  }
  .tog :global(svg) {
    transition: transform var(--dur-1) var(--ease);
  }
  .tog[aria-expanded='false'] :global(svg) {
    transform: rotate(-90deg);
  }
  .jk {
    color: var(--syn-key);
  }
  .jp {
    color: var(--syn-punct);
  }
  .jsum {
    margin: 0 4px;
    color: var(--ink-3);
    font-style: italic;
  }
  .jv {
    margin: 0 -3px;
    padding: 0 3px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    font: inherit;
  }
  button.jv {
    cursor: pointer;
  }
  button.jv:hover,
  button.jv[aria-expanded='true'] {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--accent) 40%, transparent);
  }
  .jv[data-t='string'] {
    color: var(--syn-string);
  }
  .jv[data-t='number'] {
    color: var(--syn-number);
  }
  .jv[data-t='boolean'] {
    color: var(--syn-bool);
  }
  .jv[data-t='null'] {
    color: var(--syn-null);
    font-style: italic;
  }
  .tick {
    margin-left: 6px;
    color: var(--st-pass-ink);
    font-size: 11px;
  }
  .pick {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    width: max-content;
    max-width: min(420px, 100%);
    margin: 4px 0 8px;
    padding: var(--sp-3);
    border-radius: var(--r-lg);
    white-space: normal;
    font-family: var(--font);
  }
  .pick-path {
    margin: 0;
    color: var(--ink-2);
    font-size: var(--fs-1);
    overflow-wrap: anywhere;
  }
  .pick-ops {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2);
  }
  .pick-problem {
    margin: 0;
    color: var(--st-fail-ink);
    font-size: var(--fs-1);
  }
</style>

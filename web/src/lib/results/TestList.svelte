<script lang="ts">
  import { listMove, scrollFor, windowOf } from './list.ts';
  import { ms, statusClass } from './status.ts';
  import type { TestRow } from './types.ts';

  /** Height of one row in px; the list renders only the rows in view (10,000 tests stay fast). */
  const ROW = 32;

  let {
    label,
    rows,
    current,
    onopen,
  }: {
    label: string;
    rows: readonly TestRow[];
    /** The key of the open test. */
    current: string | undefined;
    onopen: (key: string) => void;
  } = $props();

  const uid = $props.id();
  let viewport: HTMLDivElement | undefined = $state();
  let top = $state(0);
  let height = $state(0);
  let active = $state(0);
  const at = $derived(Math.min(active, rows.length - 1));
  const shown = $derived(windowOf(top, height || 600, rows.length, ROW));

  function activate(index: number): void {
    active = index;
    if (!viewport) return;
    viewport.scrollTop = scrollFor(index, viewport.scrollTop, viewport.clientHeight, ROW);
    top = viewport.scrollTop;
  }

  /** One click handler on the listbox; the option clicked carries its row index. */
  function onclick(event: MouseEvent): void {
    const option = (event.target as Element).closest<HTMLElement>('[role="option"]');
    const row = option ? rows[Number(option.dataset['index'])] : undefined;
    if (!option || !row) return;
    active = Number(option.dataset['index']);
    onopen(row.key);
  }

  function onkeydown(event: KeyboardEvent): void {
    const row = rows[at];
    if ((event.key === 'Enter' || event.key === ' ') && row) {
      event.preventDefault();
      onopen(row.key);
      return;
    }
    const page = Math.max(1, Math.floor(height / ROW) - 1);
    const next = listMove(event.key, at, rows.length, page);
    if (next === undefined) return;
    event.preventDefault();
    activate(next);
  }
</script>

<div
  class="viewport"
  role="listbox"
  aria-label={label}
  tabindex="0"
  aria-activedescendant={rows.length ? `${uid}-${at}` : undefined}
  bind:this={viewport}
  bind:clientHeight={height}
  onscroll={() => (top = viewport?.scrollTop ?? 0)}
  {onclick}
  {onkeydown}
>
  <div class="spacer" style:height="{rows.length * ROW}px">
    <div class="rows" style:transform="translateY({shown.first * ROW}px)">
      {#each rows.slice(shown.first, shown.last) as row, i (row.key)}
        {@const index = shown.first + i}
        <div
          id="{uid}-{index}"
          class="row"
          class:active={index === at}
          role="option"
          aria-selected={row.key === current}
          aria-posinset={index + 1}
          aria-setsize={rows.length}
          data-index={index}
          title={row.label}
        >
          <span class="name">
            <span class="dot {statusClass[row.status]}" aria-hidden="true"></span>
            <span class="text">{row.label}</span>
            <span class="sr">, {row.status}</span>
          </span>
          <span class="file mono">{row.file}</span>
          <span class="num">{ms(row.ms)}</span>
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .viewport {
    position: relative;
    overflow: auto;
    height: 100%;
    min-height: 240px;
    contain: strict;
  }
  .rows {
    will-change: transform;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 0.6fr) 56px;
    gap: 10px;
    align-items: center;
    height: 32px;
    padding: 0 12px;
    border-bottom: 1px solid var(--line);
    cursor: pointer;
    font-size: 13px;
  }
  .row:hover {
    background: var(--panel2);
  }
  .row.active {
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .viewport:focus-visible .row.active {
    background: var(--panel3);
  }
  .row[aria-selected='true'] {
    background: var(--accentSoft);
    color: var(--accentInk);
  }
  .name {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .text,
  .file {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .file {
    color: var(--ink2);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--ink2);
  }
</style>

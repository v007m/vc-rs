<script lang="ts">
  import { lens } from '#lib/ui/segmented.ts';
  import {
    frameBody,
    frameKind,
    frameTime,
    shownFrames,
    type FrameView,
    type StreamSession,
  } from './stream.svelte.ts';
  import { size } from './wire.ts';

  /**
   * The messages of an SSE or WebSocket stream (D564), newest last: filter them, show one
   * direction, pause to hold new ones back. A WebSocket sends text from the composer; SSE only
   * listens.
   */
  let { stream, websocket }: { stream: StreamSession; websocket: boolean } = $props();

  let direction: FrameView['direction'] = $state('all');
  let filter = $state('');
  let draft = $state('');
  const frames = $derived(shownFrames(stream.frames, { direction, filter }));
  const directions: readonly { id: FrameView['direction']; label: string }[] = [
    { id: 'all', label: 'All' },
    { id: 'in', label: 'In' },
    { id: 'out', label: 'Out' },
  ];
  const states = {
    idle: ['Not connected', 'vc-skipped'],
    connecting: ['Connecting…', 'is-live'],
    open: ['Open', 'vc-pass'],
    ended: ['Closed', 'vc-skipped'],
  } as const;
  const conn = $derived(
    stream.status === 'ended' && (stream.ended?.failed || stream.error)
      ? (['Failed', 'vc-fail'] as const)
      : states[stream.status],
  );

  async function send(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (draft.trim() === '') return;
    await stream.send(draft);
    if (!stream.error) draft = '';
  }
</script>

<section class="st-card st-card--glass" aria-label="Messages">
  <div class="card-head">
    <p class="resp-meta vc-grow">
      <span class="st-dot {conn[1]}" aria-hidden="true"></span><b>{conn[0]}</b>
      <span>{stream.frames.length} {stream.frames.length === 1 ? 'message' : 'messages'}</span>
      {#if stream.held.length}<span>{stream.held.length} held</span>{/if}
    </p>
    <button
      type="button"
      class="st-btn st-btn--sm"
      aria-pressed={stream.paused}
      onclick={() => stream.setPaused(!stream.paused)}>{stream.paused ? 'Resume' : 'Pause'}</button
    >
    <button
      type="button"
      class="st-btn st-btn--sm"
      disabled={stream.frames.length === 0}
      onclick={() => stream.clear()}>Clear</button
    >
  </div>
  <div class="body-tools pad">
    {#if websocket}
      <div class="st-segmented" role="group" {@attach lens} aria-label="Direction">
        {#each directions as d (d.id)}
          <button type="button" aria-pressed={direction === d.id} onclick={() => (direction = d.id)}
            >{d.label}</button
          >
        {/each}
      </div>
    {/if}
    <input
      class="st-input"
      type="search"
      autocomplete="off"
      aria-label="Filter messages"
      placeholder="Filter messages"
      bind:value={filter}
    />
  </div>
  {#if stream.error}
    <p class="note error pad" role="alert">{stream.error}</p>
  {/if}
  <ol class="msgs" aria-label="Stream messages" aria-live="polite" aria-relevant="additions">
    {#each frames as frame (frame.n)}
      <li class="msg" data-dir={frame.direction}>
        <details>
          <summary>
            <span class="arrow" aria-hidden="true">{frame.direction === 'in' ? '↓' : '↑'}</span>
            <span class="vc-sr">{frame.direction === 'in' ? 'Received' : 'Sent'}</span>
            <span class="vc-mono at">{frameTime(frame.atMs)}</span>
            <span class="kind vc-mono">{frameKind(frame.json)}</span>
            <span class="vc-mono size">{size(frame.size)}</span>
          </summary>
          <pre class="vc-mono">{frameBody(frame.json)}</pre>
        </details>
      </li>
    {:else}
      <li class="empty">
        {stream.status === 'idle'
          ? 'Connect to read the stream.'
          : filter || direction !== 'all'
            ? 'No message matches.'
            : 'No messages yet.'}
      </li>
    {/each}
  </ol>
  {#if stream.ended}
    <p class="note pad" role="status">
      Ended: {stream.ended.reason}{stream.ended.code === undefined ? '' : ` (${stream.ended.code})`}
    </p>
  {/if}
  {#if websocket}
    <form class="composer pad" onsubmit={send}>
      <label class="st-section-label" for="ws-draft">Message</label>
      <textarea
        id="ws-draft"
        class="st-input st-input--rect vc-mono"
        rows="3"
        spellcheck="false"
        disabled={stream.status !== 'open'}
        placeholder={'{ "type": "ping" }'}
        bind:value={draft}></textarea>
      <button
        type="submit"
        class="st-btn st-btn--primary st-btn--sm"
        disabled={stream.status !== 'open'}>Send message</button
      >
    </form>
  {/if}
</section>

<style>
  .pad {
    padding: 0 var(--st-card-padding) var(--st-space-12);
  }
  .note {
    margin: 0;
    color: var(--st-text-secondary);
  }
  .error {
    color: var(--vc-fail-ink);
  }
  .msgs {
    flex: 1 1 auto;
    min-height: 0;
    margin: 0;
    padding: 0 var(--st-card-padding) var(--st-space-12);
    overflow: auto;
    list-style: none;
    border-top: 1px solid var(--st-chrome-separator);
  }
  .msg {
    border-bottom: 1px solid var(--st-chrome-separator);
  }
  summary {
    display: flex;
    align-items: center;
    gap: var(--st-space-12);
    min-height: 32px;
    cursor: pointer;
    font-size: var(--vc-size-body);
  }
  .arrow {
    width: 14px;
    color: var(--vc-pass-ink);
    text-align: center;
  }
  .msg[data-dir='out'] .arrow {
    color: var(--st-accent);
  }
  .at,
  .size {
    color: var(--st-text-tertiary);
    font-size: var(--vc-size-caption);
  }
  .kind {
    flex: 1 1 auto;
    min-width: 0;
  }
  pre {
    margin: 0 0 var(--st-space-8) 26px;
    font-size: var(--vc-size-caption);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .empty {
    padding: var(--st-space-16) 0;
    color: var(--st-text-secondary);
  }
  .composer {
    display: grid;
    gap: var(--st-space-8);
    justify-items: end;
    padding-top: var(--st-space-12);
    border-top: 1px solid var(--st-chrome-separator);
  }
  .composer label,
  .composer textarea {
    justify-self: stretch;
  }
</style>

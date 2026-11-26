<script lang="ts">
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
    idle: ['Not connected', 'st-skipped'],
    connecting: ['Connecting…', 'is-live'],
    open: ['Open', 'st-pass'],
    ended: ['Closed', 'st-skipped'],
  } as const;
  const conn = $derived(
    stream.status === 'ended' && (stream.ended?.failed || stream.error)
      ? (['Failed', 'st-fail'] as const)
      : states[stream.status],
  );

  async function send(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (draft.trim() === '') return;
    await stream.send(draft);
    if (!stream.error) draft = '';
  }
</script>

<section class="glass" aria-label="Messages">
  <div class="card-head">
    <p class="resp-meta grow">
      <span class="dot {conn[1]}" aria-hidden="true"></span><b>{conn[0]}</b>
      <span>{stream.frames.length} {stream.frames.length === 1 ? 'message' : 'messages'}</span>
      {#if stream.held.length}<span>{stream.held.length} held</span>{/if}
    </p>
    <button
      type="button"
      class="btn btn-sm"
      aria-pressed={stream.paused}
      onclick={() => stream.setPaused(!stream.paused)}>{stream.paused ? 'Resume' : 'Pause'}</button
    >
    <button
      type="button"
      class="btn btn-sm"
      disabled={stream.frames.length === 0}
      onclick={() => stream.clear()}>Clear</button
    >
  </div>
  <div class="body-tools pad">
    {#if websocket}
      <div class="seg" role="group" aria-label="Direction">
        {#each directions as d (d.id)}
          <button type="button" aria-pressed={direction === d.id} onclick={() => (direction = d.id)}
            >{d.label}</button
          >
        {/each}
      </div>
    {/if}
    <input
      class="input"
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
            <span class="sr">{frame.direction === 'in' ? 'Received' : 'Sent'}</span>
            <span class="mono at">{frameTime(frame.atMs)}</span>
            <span class="kind mono">{frameKind(frame.json)}</span>
            <span class="mono size">{size(frame.size)}</span>
          </summary>
          <pre class="mono">{frameBody(frame.json)}</pre>
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
      <label class="section-label" for="ws-draft">Message</label>
      <textarea
        id="ws-draft"
        class="textarea mono"
        rows="3"
        spellcheck="false"
        disabled={stream.status !== 'open'}
        placeholder={'{ "type": "ping" }'}
        bind:value={draft}></textarea>
      <button type="submit" class="btn primary btn-sm" disabled={stream.status !== 'open'}
        >Send message</button
      >
    </form>
  {/if}
</section>

<style>
  .pad {
    padding: 0 var(--card-pad) var(--sp-3);
  }
  .note {
    margin: 0;
    color: var(--ink-2);
  }
  .error {
    color: var(--st-fail-ink);
  }
  .msgs {
    flex: 1 1 auto;
    min-height: 0;
    margin: 0;
    padding: 0 var(--card-pad) var(--sp-3);
    overflow: auto;
    list-style: none;
    border-top: 1px solid var(--rule);
  }
  .msg {
    border-bottom: 1px solid var(--rule);
  }
  summary {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-height: 32px;
    cursor: pointer;
    font-size: var(--fs-2);
  }
  .arrow {
    width: 14px;
    color: var(--st-pass-ink);
    text-align: center;
  }
  .msg[data-dir='out'] .arrow {
    color: var(--accent);
  }
  .at,
  .size {
    color: var(--ink-3);
    font-size: var(--fs-1);
  }
  .kind {
    flex: 1 1 auto;
    min-width: 0;
  }
  pre {
    margin: 0 0 var(--sp-2) 26px;
    font-size: var(--fs-1);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .empty {
    padding: var(--sp-4) 0;
    color: var(--ink-2);
  }
  .composer {
    display: grid;
    gap: var(--sp-2);
    justify-items: end;
    padding-top: var(--sp-3);
    border-top: 1px solid var(--rule);
  }
  .composer label,
  .composer textarea {
    justify-self: stretch;
  }
</style>

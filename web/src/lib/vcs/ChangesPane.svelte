<script lang="ts">
  import Menu, { type MenuItem } from '#lib/ui/Menu.svelte';
  import GitDiffDialog from './GitDiffDialog.svelte';
  import { letters, type ChangedFile } from './port.ts';
  import type { VcsSession } from './session.svelte.ts';

  /**
   * The Tests sidebar's Changes section (D518): the branch with its commits ahead and behind, the
   * changed files (pick, open the diff), a message and Commit; Push, Pull and Commit & push sit in
   * the menu. What git said shows under it, its failures in git's own words.
   */
  let { vcs }: { vcs: VcsSession } = $props();

  let open: ChangedFile | undefined = $state();
  const status = $derived(vcs.status?.available ? vcs.status.value : undefined);
  const count = $derived(vcs.picked.length);
  const items: MenuItem[] = $derived([
    { id: 'push', label: 'Push', meta: status?.ahead ? `${status.ahead} ahead` : 'up to date' },
    { id: 'pull', label: 'Pull', meta: status?.behind ? `${status.behind} behind` : undefined },
    { id: 'commit-push', label: 'Commit & push', disabled: !vcs.canCommit },
    { id: 'copy', label: 'Copy branch name', disabled: !vcs.branch },
  ]);

  function plural(n: number, word: string): string {
    return `${n} ${word}${n === 1 ? '' : 's'}`;
  }

  function pick(id: string): void {
    if (id === 'push') void vcs.push();
    else if (id === 'pull') void vcs.pull();
    else if (id === 'commit-push') void vcs.commitAndPush();
    else if (vcs.branch) void navigator.clipboard.writeText(vcs.branch);
  }
</script>

{#if vcs.status && !vcs.status.available}
  <p class="tg-empty">{vcs.status.reason}</p>
{:else if status}
  <div class="tg">
    <div class="tg-branch" title={status.upstream ? `Tracking ${status.upstream}` : 'No upstream'}>
      <span class="vc-mono ellipsis vc-grow">{status.branch || 'detached HEAD'}</span>
      <span
        class="tg-ab num"
        aria-label="{plural(status.ahead, 'commit')} to push, {plural(
          status.behind,
          'commit',
        )} to pull"
      >
        <span class:is-on={status.ahead > 0} aria-hidden="true">↑{status.ahead}</span>
        <span class:is-on={status.behind > 0} aria-hidden="true">↓{status.behind}</span>
      </span>
    </div>
    {#if status.files.length}
      <label class="tg-head">
        <input
          type="checkbox"
          class="st-check"
          checked={count === status.files.length}
          indeterminate={count > 0 && count < status.files.length}
          onchange={() => vcs.toggleAll()}
          aria-label="Pick all {status.files.length} files"
        />
        <span>{count} of {status.files.length} picked</span>
      </label>
      <ul class="tg-files" aria-label="Changed files">
        {#each status.files as file (file.path)}
          {@const name = file.path.slice(file.path.lastIndexOf('/') + 1)}
          <li class="tg-file" class:is-unpicked={!vcs.picked.includes(file.path)}>
            <input
              type="checkbox"
              class="st-check"
              checked={vcs.picked.includes(file.path)}
              onchange={() => vcs.toggle(file.path)}
              aria-label="Pick {file.path}"
            />
            <button type="button" class="tg-open" title={file.path} onclick={() => (open = file)}>
              <span class="tg-st tg-st-{letters[file.change]}" aria-label={file.change}
                >{letters[file.change]}</span
              >
              <span class="tg-name ellipsis">{name}</span>
              {#if file.lines}
                <span class="tg-n num">
                  <span class="add">+{file.lines.added}</span>
                  <span class="del">−{file.lines.removed}</span>
                </span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
      <textarea
        class="st-input st-input--rect tg-text"
        rows="2"
        placeholder="Commit message"
        aria-label="Commit message"
        bind:value={vcs.message}></textarea>
    {:else}
      <p class="tg-empty">No changes to commit.</p>
    {/if}
    <div class="tg-acts">
      {#if status.files.length}
        <button
          type="button"
          class="st-btn st-btn--primary st-btn--sm vc-grow"
          disabled={!vcs.canCommit}
          onclick={() => void vcs.commit()}
        >
          Commit {count ? plural(count, 'file') : ''}
        </button>
      {:else if status.ahead}
        <button
          type="button"
          class="st-btn st-btn--primary st-btn--sm vc-grow"
          onclick={() => void vcs.push()}
        >
          Push {plural(status.ahead, 'commit')}
        </button>
      {:else}
        <span class="vc-grow"></span>
      {/if}
      <Menu label="More git actions" {items} button="st-icon-btn st-icon-btn--sm" onpick={pick}>
        <span aria-hidden="true">⋯</span>
      </Menu>
    </div>
    <div role="status" class="tg-note" class:fail={vcs.note?.tone === 'fail'}>
      {#if vcs.busy}Working…{:else if vcs.note}{vcs.note.text}{/if}
    </div>
    {#if !vcs.busy && vcs.note?.output}
      <pre class="tg-out vc-mono" aria-label="What git said">{vcs.note.output}</pre>
    {/if}
  </div>
{:else}
  <p class="tg-empty" role="status">Reading git status…</p>
{/if}

{#if open}
  <GitDiffDialog {vcs} file={open} onclose={() => (open = undefined)} />
{/if}

<style>
  .tg {
    display: flex;
    flex-direction: column;
    gap: var(--st-space-8);
    min-width: 0;
    padding: 0 var(--st-space-4) var(--st-space-12);
    font-size: var(--vc-size-body);
    color: var(--st-text-primary);
  }
  .tg-branch {
    display: flex;
    align-items: center;
    gap: var(--st-space-8);
    min-width: 0;
    min-height: 28px;
    padding: 0 var(--st-space-8);
    color: var(--st-text-secondary);
    font-size: var(--vc-size-caption);
  }
  .tg-ab {
    display: inline-flex;
    flex: none;
    gap: var(--st-space-8);
    color: var(--st-text-tertiary);
  }
  .tg-ab .is-on {
    color: var(--st-text-primary);
    font-weight: var(--st-weight-semibold);
  }
  .tg-head {
    display: flex;
    align-items: center;
    gap: var(--st-space-8);
    min-height: 28px;
    padding: 0 var(--st-space-8);
    color: var(--st-text-tertiary);
    font-size: var(--vc-size-caption);
    cursor: pointer;
  }
  .tg-files {
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .tg-file {
    display: flex;
    align-items: center;
    gap: var(--st-space-4);
    min-height: 30px;
    padding-left: var(--st-space-8);
    border-radius: var(--st-radius-sm);
  }
  .tg-file:hover {
    background: var(--st-chrome-hover);
  }
  .tg-file input,
  .tg-head input {
    flex: none;
    margin: 0;
  }
  .tg-open {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    gap: var(--st-space-8);
    min-width: 0;
    height: 28px;
    padding: 0 var(--st-space-8) 0 var(--st-space-4);
    border: 0;
    border-radius: var(--st-radius-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .tg-open:focus-visible {
    outline: 2px solid var(--st-accent);
    outline-offset: -2px;
  }
  .is-unpicked .tg-name {
    color: var(--st-text-tertiary);
  }
  .tg-name {
    flex: 1 1 auto;
    min-width: 0;
  }
  .tg-st {
    flex: none;
    width: 12px;
    font-family: var(--st-font-mono);
    font-size: 11px;
    font-weight: var(--st-weight-semibold);
    text-align: center;
  }
  .tg-st-M,
  .tg-st-C {
    color: var(--vc-blocked-ink);
  }
  .tg-st-A,
  .tg-st-U {
    color: var(--vc-diff-add-ink);
  }
  .tg-st-D {
    color: var(--vc-diff-del-ink);
  }
  .tg-n {
    display: inline-flex;
    flex: none;
    gap: var(--st-space-4);
    font-family: var(--st-font-mono);
    font-size: 11px;
  }
  .add {
    color: var(--vc-diff-add-ink);
  }
  .del {
    color: var(--vc-diff-del-ink);
  }
  .tg-text {
    min-height: 56px;
    resize: vertical;
  }
  .tg-acts {
    display: flex;
    align-items: center;
    gap: var(--st-space-8);
  }
  .tg-empty {
    margin: 0;
    padding: var(--st-space-8);
    color: var(--st-text-secondary);
  }
  .tg-note {
    padding: 0 var(--st-space-8);
    color: var(--st-text-secondary);
    font-size: var(--vc-size-caption);
  }
  .tg-note:empty {
    display: none;
  }
  .tg-note.fail {
    color: var(--vc-fail-ink);
  }
  .ellipsis {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .tg-out {
    max-height: 160px;
    overflow: auto;
    margin: 0;
    padding: var(--st-space-8);
    border-radius: var(--st-radius-sm);
    background: var(--st-surface-input-on-card);
    font-size: 11px;
    white-space: pre-wrap;
  }
</style>

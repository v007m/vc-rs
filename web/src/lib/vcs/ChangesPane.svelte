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
      <span class="mono ellipsis grow">{status.branch || 'detached HEAD'}</span>
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
        class="textarea tg-text"
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
          class="btn btn-primary btn-sm grow"
          disabled={!vcs.canCommit}
          onclick={() => void vcs.commit()}
        >
          Commit {count ? plural(count, 'file') : ''}
        </button>
      {:else if status.ahead}
        <button type="button" class="btn btn-primary btn-sm grow" onclick={() => void vcs.push()}>
          Push {plural(status.ahead, 'commit')}
        </button>
      {:else}
        <span class="grow"></span>
      {/if}
      <Menu label="More git actions" {items} button="btn btn-ghost btn-icon btn-sm" onpick={pick}>
        <span aria-hidden="true">⋯</span>
      </Menu>
    </div>
    <div role="status" class="tg-note" class:fail={vcs.note?.tone === 'fail'}>
      {#if vcs.busy}Working…{:else if vcs.note}{vcs.note.text}{/if}
    </div>
    {#if !vcs.busy && vcs.note?.output}
      <pre class="tg-out mono" aria-label="What git said">{vcs.note.output}</pre>
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
    gap: var(--sp-2);
    min-width: 0;
    padding: 0 var(--sp-1) var(--sp-3);
    font-size: var(--fs-2);
    color: var(--l0-ink);
  }
  .tg-branch {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-width: 0;
    min-height: 28px;
    padding: 0 var(--sp-2);
    color: var(--l0-ink-2);
    font-size: var(--fs-1);
  }
  .tg-ab {
    display: inline-flex;
    flex: none;
    gap: var(--sp-2);
    color: var(--l0-ink-3);
  }
  .tg-ab .is-on {
    color: var(--l0-ink);
    font-weight: var(--fw-semibold);
  }
  .tg-head {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 28px;
    padding: 0 var(--sp-2);
    color: var(--l0-ink-3);
    font-size: var(--fs-1);
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
    gap: var(--sp-1);
    min-height: 30px;
    padding-left: var(--sp-2);
    border-radius: var(--r-sm);
  }
  .tg-file:hover {
    background: var(--l0-hover);
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
    gap: var(--sp-2);
    min-width: 0;
    height: 28px;
    padding: 0 var(--sp-2) 0 var(--sp-1);
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .tg-open:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .is-unpicked .tg-name {
    color: var(--l0-ink-3);
  }
  .tg-name {
    flex: 1 1 auto;
    min-width: 0;
  }
  .tg-st {
    flex: none;
    width: 12px;
    font-family: var(--mono);
    font-size: 11px;
    font-weight: var(--fw-semibold);
    text-align: center;
  }
  .tg-st-M,
  .tg-st-C {
    color: var(--st-blocked-ink);
  }
  .tg-st-A,
  .tg-st-U {
    color: var(--diff-add-ink);
  }
  .tg-st-D {
    color: var(--diff-del-ink);
  }
  .tg-n {
    display: inline-flex;
    flex: none;
    gap: var(--sp-1);
    font-family: var(--mono);
    font-size: 11px;
  }
  .add {
    color: var(--diff-add-ink);
  }
  .del {
    color: var(--diff-del-ink);
  }
  .tg-text {
    min-height: 56px;
    resize: vertical;
  }
  .tg-acts {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .tg-empty {
    margin: 0;
    padding: var(--sp-2);
    color: var(--l0-ink-2);
  }
  .tg-note {
    padding: 0 var(--sp-2);
    color: var(--l0-ink-2);
    font-size: var(--fs-1);
  }
  .tg-note:empty {
    display: none;
  }
  .tg-note.fail {
    color: var(--st-fail-ink);
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
    padding: var(--sp-2);
    border-radius: var(--r-sm);
    background: var(--well-bg);
    font-size: 11px;
    white-space: pre-wrap;
  }
</style>

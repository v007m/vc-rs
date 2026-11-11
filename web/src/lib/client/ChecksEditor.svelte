<script lang="ts">
  import { PHASES, checkText, expectLines, type CheckKind, type CheckOp } from './apiForm.ts';
  import type { ApiRequest } from './request.svelte.ts';

  /** The api's own checks, read-only, and the checks added for this request alone. */
  let { request }: { request: ApiRequest } = $props();

  const kinds: readonly { id: CheckKind; label: string }[] = [
    { id: 'status', label: 'Status' },
    { id: 'body', label: 'Body field' },
    { id: 'timing', label: 'Timing' },
  ];
  const ops: readonly CheckOp[] = ['equals', 'contains', 'exists', 'lt', 'gt'];

  let kind: CheckKind = $state('status');
  let path = $state('');
  let op: CheckOp = $state('equals');
  let value = $state('');
  let problem: string | undefined = $state();
  const own = $derived(request.written ? expectLines(request.written.node) : []);

  function picked(): void {
    path = kind === 'timing' ? 'total' : '';
    problem = undefined;
  }

  function add(event: SubmitEvent): void {
    event.preventDefault();
    problem = request.addCheck({ kind, path, op, value });
    if (problem === undefined) value = '';
  }
</script>

<p class="hint">From the api's <code>expect</code>. Read-only here; edit them in the api file.</p>
<ul class="list mono" aria-label="Api checks">
  {#each own as line (line)}
    <li>{line}</li>
  {:else}
    <li class="hint">The api has no expect block.</li>
  {/each}
</ul>

<h3 class="h3">Added for this request</h3>
<ul class="list" aria-label="Added checks">
  {#each request.checks as check, i (i)}
    <li>
      <span class="mono grow">{checkText(check)}</span>
      <button
        type="button"
        class="btn sm"
        aria-label="Remove {checkText(check)}"
        onclick={() => request.removeCheck(i)}>Remove</button
      >
    </li>
  {/each}
</ul>
<form class="add" onsubmit={add}>
  <label>
    <span class="sr">Check</span>
    <select class="input" bind:value={kind} onchange={picked}>
      {#each kinds as k (k.id)}<option value={k.id}>{k.label}</option>{/each}
    </select>
  </label>
  {#if kind === 'body'}
    <label>
      <span class="sr">Body path</span>
      <input class="input mono" placeholder="$.id" spellcheck="false" bind:value={path} />
    </label>
  {:else if kind === 'timing'}
    <label>
      <span class="sr">Phase</span>
      <select class="input mono" bind:value={path}>
        {#each PHASES as phase (phase)}<option value={phase}>{phase}</option>{/each}
      </select>
    </label>
  {/if}
  <label>
    <span class="sr">Matcher</span>
    <select class="input mono" bind:value={op}>
      {#each ops as o (o)}<option value={o}>{o}</option>{/each}
    </select>
  </label>
  <label>
    <span class="sr">Expected value</span>
    <input
      class="input mono"
      placeholder={kind === 'timing' ? '500ms' : kind === 'status' ? '201' : 'value'}
      spellcheck="false"
      aria-invalid={problem !== undefined}
      aria-describedby={problem ? 'check-problem' : undefined}
      bind:value
    />
  </label>
  <button type="submit" class="btn sm">Add check</button>
  {#if problem}<p class="error" id="check-problem">{problem}</p>{/if}
</form>
<p class="hint">Added checks run on every send of this request and are never saved to the api.</p>

<style>
  .hint {
    margin: 4px 0;
    color: var(--ink2);
  }
  .h3 {
    margin: 14px 0 4px;
    font-size: 13px;
    font-weight: 600;
  }
  .list {
    display: grid;
    gap: 4px;
    margin: 4px 0;
    padding: 0;
    list-style: none;
  }
  .list li,
  .add {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .error {
    flex-basis: 100%;
    margin: 0;
    color: var(--fail);
  }
</style>

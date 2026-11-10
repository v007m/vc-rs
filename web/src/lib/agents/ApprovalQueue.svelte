<script lang="ts">
  import { Verdict, type Approval } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
  import { ago, type AgentDesk } from './desk.svelte.ts';

  let { desk, now }: { desk: AgentDesk; now: number } = $props();

  /** The approval whose "Always allow" is asking for confirmation. */
  let confirming: bigint | undefined = $state();

  function decide(approval: Approval, verdict: Verdict): void {
    confirming = undefined;
    void desk.decide(approval.id, verdict);
  }
</script>

<section class="queue" aria-labelledby="queue-h">
  <h2 id="queue-h" class="h2">Waiting for you</h2>
  <p class="lead">
    The agent is paused until you decide. With no browser open, <code>vc mcp</code> asks in its terminal
    instead.
  </p>
  {#each desk.pending as approval (approval.id)}
    {@const busy = desk.deciding === approval.id}
    <article class="card" aria-labelledby="ask-{approval.id}">
      <h3 id="ask-{approval.id}" class="ask">
        <strong>{approval.agent}</strong> wants to <span class="mono">{approval.tool}</span> on
        <span class="pill accent">{approval.profile}</span>
      </h3>
      <p class="meta">
        {#each approval.methods as method (method)}<span class="pill mono">{method}</span>{/each}
        <span>Asked {ago(approval.createdMs, now)}</span>
      </p>
      <p>{approval.summary}</p>
      <details>
        <summary>Arguments</summary>
        <pre class="mono">{approval.arguments}</pre>
      </details>
      {#if confirming === approval.id}
        <div class="confirm" role="group" aria-label="Confirm always allow">
          <p>
            Always allow saves a rule to <code>vibranium.config.yaml</code>, so later
            <span class="mono">{approval.tool}</span> calls on {approval.profile} run without asking.
            You can change it back in Settings.
          </p>
          <button
            class="btn primary sm"
            type="button"
            disabled={busy}
            onclick={() => decide(approval, Verdict.ALWAYS_ALLOW)}
          >
            Save rule and approve
          </button>
          <button class="btn sm" type="button" onclick={() => (confirming = undefined)}
            >Cancel</button
          >
        </div>
      {:else}
        <div class="actions">
          <button
            class="btn primary sm"
            type="button"
            disabled={busy}
            onclick={() => decide(approval, Verdict.APPROVE)}
          >
            Approve once
          </button>
          <button
            class="btn sm"
            type="button"
            disabled={busy}
            onclick={() => (confirming = approval.id)}
          >
            Always allow on {approval.profile}
          </button>
          <button
            class="btn sm"
            type="button"
            disabled={busy}
            onclick={() => decide(approval, Verdict.DENY)}
          >
            Deny
          </button>
        </div>
      {/if}
    </article>
  {:else}
    <div class="empty">
      <p><strong>Nothing waiting.</strong></p>
      <p>Agents pause here when they need you. Your decisions are in the activity log below.</p>
    </div>
  {/each}
</section>

<style>
  .queue {
    display: grid;
    gap: 12px;
  }
  .lead,
  .meta,
  .empty p {
    margin: 0;
    color: var(--ink2);
  }
  .card {
    display: grid;
    gap: 8px;
    padding: 16px;
    border: 1px solid var(--line2);
    border-radius: var(--radius);
    background: var(--panel);
  }
  .card p {
    margin: 0;
  }
  .ask {
    margin: 0;
    font-size: 15px;
    font-weight: 400;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  pre {
    margin: 8px 0 0;
    padding: 12px;
    border-radius: 8px;
    background: var(--panel2);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .actions,
  .confirm {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .confirm p {
    flex-basis: 100%;
  }
</style>

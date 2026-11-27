<script lang="ts">
  import { resolve } from '$app/paths';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import { getToasts, type Toasts } from '#lib/ui/toasts.svelte.ts';
  import { sentText, type Alerts } from './alerts.svelte.ts';

  /** The config's alert rules, read only, and a test alert sent on their channels (D713). */
  let {
    alerts,
    profile,
    toasts = getToasts(),
  }: { alerts: Alerts; profile: string | undefined; toasts?: Toasts } = $props();

  const uid = $props.id();
  const editorHref = `${resolve('/tests/editor')}?file=vibranium.config.yaml`;
  let under = $derived(profile || 'the default profile');

  async function send(): Promise<void> {
    try {
      const sent = await alerts.test(profile ?? '');
      if (sent) toasts.show(sentText(sent));
    } catch (error) {
      toasts.fail(error);
    }
  }
</script>

<div class="alerts">
  <section class="part" aria-labelledby="{uid}-rules">
    <h2 id="{uid}-rules" class="st-title3">Alert rules</h2>
    <p class="intro">
      The rules under <code>alerts</code> in <code>vibranium.config.yaml</code>, as the file writes
      them. To change them, edit the config.
      <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- editorHref is resolve('/tests/editor') plus a query -->
      <a href={editorHref}>Open it in the editor</a>
    </p>
    {#if alerts.error}<p class="error" role="alert">{alerts.error}</p>{/if}
    {#if alerts.rules.length > 0}
      <table class="vc-table">
        <caption class="vc-sr">Alert rules</caption>
        <thead>
          <tr><th scope="col">Rule</th><th scope="col">As written</th></tr>
        </thead>
        <tbody>
          {#each alerts.rules as rule, i (i)}
            <tr>
              <th scope="row">Rule {i + 1}</th>
              <td><pre class="vc-mono">{rule}</pre></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else if alerts.loaded && !alerts.error}
      <EmptyState
        icon="gear"
        title="No alert rules"
        caption="Add rules under alerts in vibranium.config.yaml to be told when a run fails."
      />
    {/if}
  </section>

  <section class="part" aria-labelledby="{uid}-test">
    <h2 id="{uid}-test" class="st-title3">Test alert</h2>
    {#if alerts.missing}
      <EmptyState icon="send" title="Test alerts are not available" caption={alerts.missing} />
    {:else}
      <div class="send">
        <p class="intro">
          Sends a test message on every channel the rules name under <strong>{under}</strong>.
          Nothing runs and nothing is recorded.
        </p>
        <button
          type="button"
          class="st-btn st-btn--primary"
          disabled={alerts.sending}
          onclick={() => void send()}>{alerts.sending ? 'Sending…' : 'Send a test alert'}</button
        >
      </div>
      {#if alerts.sent?.length === 0}
        <p class="intro">No rule applies under {under}, so nothing was sent.</p>
      {:else if alerts.sent}
        <table class="vc-table">
          <caption class="vc-sr">Test alert results</caption>
          <thead>
            <tr>
              <th scope="col">Rule</th><th scope="col">Channel</th><th scope="col">Result</th>
            </tr>
          </thead>
          <tbody>
            {#each alerts.sent as one, i (i)}
              <tr>
                <th scope="row">Rule {one.rule}</th>
                <td class="vc-mono">{one.channel}</td>
                <td>
                  <span class="result">
                    <span
                      class="st-dot {one.error === undefined ? 'vc-pass' : 'vc-fail'}"
                      aria-hidden="true"
                    ></span>
                    {#if one.error === undefined}Sent{:else}<span class="failed"
                        >Failed: {one.error}</span
                      >{/if}
                  </span>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    {/if}
  </section>
</div>

<style>
  .alerts {
    display: grid;
    gap: 28px;
  }
  .part {
    display: grid;
    gap: 12px;
  }
  .intro {
    margin: 0;
    color: var(--st-text-secondary);
  }
  .send {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
    justify-content: space-between;
  }
  .send .intro {
    flex: 1 1 320px;
  }
  .vc-table th,
  .vc-table td {
    vertical-align: top;
  }
  pre {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .result {
    display: inline-flex;
    align-items: baseline;
    gap: 8px;
  }
  .failed {
    color: var(--vc-fail-ink);
  }
  .error {
    margin: 0;
    color: var(--vc-fail-ink);
  }
</style>

<script lang="ts">
  import { resolve } from '$app/paths';
  import { MASKED } from './port.ts';
  import type { EffectiveSettings } from './settings.svelte.ts';

  let { settings }: { settings: EffectiveSettings } = $props();
  const editorHref = `${resolve('/editor')}?file=vibranium.config.yaml`;
  const maskedNote = ' (secret, masked)';
</script>

{#snippet shown(value: string)}
  {#if value === MASKED}
    <span class="masked">{MASKED}<span class="sr">{maskedNote}</span></span>
  {:else}
    {value}
  {/if}
{/snippet}

<div class="settings">
  <p class="intro">
    Where each setting's value comes from under the profile you pick, as
    <code>vc config explain</code> shows it. Change them in
    <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- editorHref is resolve('/editor') plus a query -->
    <a href={editorHref} aria-label="Open vibranium.config.yaml in the editor"
      ><code>vibranium.config.yaml</code></a
    >; secrets are declared in <code>.env.schema</code> and always show masked.
  </p>
  <label class="profile">
    Profile
    <select
      class="input"
      value={settings.profile}
      disabled={settings.profiles.length === 0}
      onchange={(e) => void settings.choose(e.currentTarget.value)}
    >
      {#each settings.profiles as profile (profile)}
        <option value={profile}>{profile}</option>
      {/each}
    </select>
  </label>
  {#if settings.error}
    <p class="error" role="alert">{settings.error}</p>
  {/if}
  {#each settings.notices as notice, i (i)}
    <p class="notice">{notice}</p>
  {/each}
  <table aria-busy={settings.loading}>
    <caption class="h2">Effective settings</caption>
    <thead>
      <tr><th scope="col">Key</th><th scope="col">Value</th><th scope="col">Set by</th></tr>
    </thead>
    <tbody>
      {#each settings.values as { key, layers } (key)}
        {@const [winner, ...replaced] = layers}
        <tr>
          <th scope="row" class="mono">{key}</th>
          <td class="mono"
            >{#if winner}{@render shown(winner.value)}{/if}</td
          >
          <td>
            {winner?.source ?? '—'}
            {#if replaced.length > 0}
              <ul class="over">
                {#each replaced as layer, i (i)}
                  <li>
                    over <span class="mono">{@render shown(layer.value)}</span> ({layer.source})
                  </li>
                {/each}
              </ul>
            {/if}
          </td>
        </tr>
      {:else}
        <tr
          ><td colspan="3" class="empty">{settings.loading ? 'Loading…' : 'Nothing to show.'}</td
          ></tr
        >
      {/each}
    </tbody>
  </table>
</div>

<style>
  .settings {
    display: grid;
    gap: 12px;
    max-width: 960px;
  }
  .intro {
    margin: 0;
    color: var(--ink2);
  }
  .profile {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--ink2);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  caption {
    text-align: left;
    margin-bottom: 8px;
  }
  th,
  td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--line);
    text-align: left;
    vertical-align: top;
  }
  thead th {
    color: var(--ink2);
    font-weight: 500;
  }
  tbody th {
    font-weight: 400;
  }
  .over {
    margin: 4px 0 0;
    padding: 0;
    list-style: none;
    font-size: 13px;
    color: var(--ink2);
  }
  .masked {
    letter-spacing: 1px;
  }
  .empty,
  .notice {
    color: var(--ink2);
  }
  .error {
    color: var(--fail);
  }
</style>

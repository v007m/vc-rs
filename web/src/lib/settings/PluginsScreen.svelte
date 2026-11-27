<script lang="ts">
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import { getToasts, type Toasts } from '#lib/ui/toasts.svelte.ts';
  import PluginsAddDialog from './PluginsAddDialog.svelte';
  import PluginsRemoveDialog from './PluginsRemoveDialog.svelte';
  import { reachTags, type Plugins, type PluginReach } from './plugins.svelte.ts';

  /** The project's plugins (D715): each one's source, reach and state, added and removed here. */
  let { plugins, toasts = getToasts() }: { plugins: Plugins; toasts?: Toasts } = $props();

  const uid = $props.id();
  let adding = $state(false);
</script>

{#snippet tags(found: PluginReach, empty: string)}
  {@const all = reachTags(found)}
  <span class="tags">
    {#each all as tag (tag)}<span class="st-badge">{tag}</span>{:else}<span class="quiet"
        >{empty}</span
      >{/each}
  </span>
{/snippet}

{#snippet addButton()}
  <button type="button" class="st-btn st-btn--primary" onclick={() => (adding = true)}
    >Add a plugin</button
  >
{/snippet}

{#if plugins.missing}
  <EmptyState icon="import" title="Plugins are not available" caption={plugins.missing} />
{:else if plugins.loaded && plugins.list.length === 0 && !plugins.error}
  <EmptyState
    icon="import"
    title="No plugins yet"
    caption="A plugin adds an importer, reporter or other kind from npm or a folder of the project."
    action={addButton}
  />
{:else}
  <section class="plugins" aria-labelledby="{uid}-title">
    <div class="head">
      <h2 id="{uid}-title" class="st-title3">Plugins</h2>
      {@render addButton()}
    </div>
    <p class="intro">
      The plugins the config and <code>vibranium.lock</code> name, what each asks to reach and what the
      lock grants it.
    </p>
    {#if plugins.error}<p class="error" role="alert">{plugins.error}</p>{/if}
    <table class="vc-table">
      <caption class="vc-sr">Plugins</caption>
      <thead>
        <tr>
          <th scope="col">Plugin</th>
          <th scope="col">Source</th>
          <th scope="col">Asks to reach</th>
          <th scope="col">Granted</th>
          <th scope="col">State</th>
          <th scope="col"><span class="vc-sr">Actions</span></th>
        </tr>
      </thead>
      <tbody>
        {#each plugins.list as plugin (plugin.id)}
          <tr>
            <th scope="row">
              <span class="vc-mono">{plugin.id}</span>
              {#if plugin.about}
                <span class="quiet">{plugin.about.version} · {plugin.about.kind}</span>
              {/if}
            </th>
            <td class="vc-mono">{plugin.source}</td>
            <td>{@render tags(plugin.asks, 'Nothing')}</td>
            <td>{@render tags(plugin.granted, 'Nothing yet')}</td>
            <td>
              <span class="st-badge {plugin.state === 'ready' ? 'vc-pass' : 'vc-blocked'}"
                >{plugin.state}</span
              >
              {#if plugin.detail && plugin.detail !== plugin.state}<span class="quiet detail"
                  >{plugin.detail}</span
                >{/if}
            </td>
            <td class="act">
              <button
                type="button"
                class="st-btn st-btn--danger st-btn--sm"
                aria-label="Remove {plugin.id}"
                onclick={() => void plugins.previewRemove(plugin.id)}>Remove</button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </section>
{/if}

{#if adding}
  <PluginsAddDialog {plugins} {toasts} onclose={() => (adding = false)} />
{/if}
{#if plugins.removal}
  <PluginsRemoveDialog {plugins} id={plugins.removal} {toasts} />
{/if}

<style>
  .plugins {
    display: grid;
    gap: 12px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .head h2 {
    margin: 0;
  }
  .intro {
    margin: 0;
    color: var(--st-text-secondary);
  }
  .vc-table th,
  .vc-table td {
    vertical-align: top;
  }
  th .quiet {
    display: block;
    margin-top: 2px;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .quiet {
    color: var(--st-text-secondary);
    font: var(--st-type-caption);
  }
  .detail {
    display: block;
    margin-top: 4px;
  }
  .act {
    width: 1%;
    white-space: nowrap;
  }
  .error {
    margin: 0;
    color: var(--vc-fail-ink);
  }
</style>

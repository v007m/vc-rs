<script lang="ts">
  import Dialog from '#lib/ui/Dialog.svelte';
  import type { Toasts } from '#lib/ui/toasts.svelte.ts';
  import ChangeList from '#lib/writes/ChangeList.svelte';
  import { reachTags, type Plugins } from './plugins.svelte.ts';

  /**
   * Add a plugin (D714): a source, its preview (what the plugin is, what it asks to reach, its
   * warnings and the config and lock changes), then Add with the preview's digest. Pressing
   * Add after reading the preview is the approval `--yes` gives at the command line.
   */
  let { plugins, toasts, onclose }: { plugins: Plugins; toasts: Toasts; onclose: () => void } =
    $props();

  const uid = $props.id();
  let source = $state('');
  let staged = $derived(plugins.adding);
  let offer = $derived(plugins.offer);
  let asks = $derived(offer ? reachTags(offer.asks) : []);

  function close(): void {
    plugins.resetAdd();
    onclose();
  }

  function review(event: SubmitEvent): void {
    event.preventDefault();
    void plugins.previewAdd(source.trim());
  }

  async function add(): Promise<void> {
    const id = offer?.about?.id;
    if (await plugins.add(source.trim())) {
      toasts.show(id ? `Added the plugin ${id}.` : 'Added the plugin.');
      onclose();
    } else if (staged.note) toasts.show(staged.note, 'error');
  }
</script>

<Dialog title="Add a plugin" onclose={close}>
  <form class="body" onsubmit={review}>
    <label class="field" for="{uid}-source">Source</label>
    <input
      id="{uid}-source"
      class="st-input vc-mono"
      autocomplete="off"
      spellcheck="false"
      placeholder="npm:@acme/vibranium-report@^2"
      aria-invalid={staged.note !== undefined}
      aria-describedby="{uid}-hint{staged.note ? ` ${uid}-note` : ''}"
      bind:value={source}
      oninput={() => plugins.resetAdd()}
    />
    <p id="{uid}-hint" class="hint">
      A package of the npm registry, <code>npm:&lt;package&gt;[@&lt;range&gt;]</code>, or a folder
      inside the project, <code>path:&lt;folder&gt;</code>.
    </p>
    {#if staged.note}<p id="{uid}-note" class="error" role="alert">{staged.note}</p>{/if}

    {#if offer}
      <section class="offer" aria-label="Preview of the plugin">
        <dl>
          {#if offer.about}
            <dt>Plugin</dt>
            <dd><span class="vc-mono">{offer.about.id}</span> {offer.about.version}</dd>
            <dt>Kind</dt>
            <dd>{offer.about.kind}</dd>
          {/if}
          <dt>Resolved</dt>
          <dd class="vc-mono">{offer.resolved}</dd>
          <dt>Integrity</dt>
          <dd class="vc-mono">{offer.integrity}</dd>
          <dt>Asks to reach</dt>
          <dd>
            {#each asks as tag (tag)}<span class="st-badge">{tag}</span>{:else}Nothing{/each}
          </dd>
        </dl>
        {#each offer.warnings as warning (warning)}
          <p class="vc-banner vc-banner--warn">{warning}</p>
        {/each}
        <ChangeList files={offer.files} />
      </section>
    {/if}

    <div class="actions">
      <button type="button" class="st-btn" onclick={close}>Cancel</button>
      {#if offer}
        <button
          type="button"
          class="st-btn st-btn--primary"
          disabled={staged.busy}
          onclick={() => void add()}>{staged.busy ? 'Adding…' : 'Add'}</button
        >
      {:else}
        <button
          type="submit"
          class="st-btn st-btn--primary"
          disabled={staged.busy || !source.trim()}>{staged.busy ? 'Fetching…' : 'Preview'}</button
        >
      {/if}
    </div>
  </form>
</Dialog>

<style>
  .body,
  .offer {
    display: grid;
    gap: 8px;
  }
  .offer {
    gap: 12px;
    margin-top: 8px;
  }
  .field {
    font: var(--st-type-caption);
    color: var(--st-text-secondary);
  }
  .hint {
    margin: 0;
    font: var(--st-type-caption);
    color: var(--st-text-secondary);
  }
  .error {
    margin: 0;
    color: var(--vc-fail-ink);
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 12px;
    margin: 0;
  }
  dt {
    color: var(--st-text-secondary);
  }
  dd {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>

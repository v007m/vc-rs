<script lang="ts">
  import { lifetime, type AuthTest } from './authTest.svelte.ts';

  let { auth, profile }: { auth: AuthTest; profile: string } = $props();
  const names = (list: readonly string[]) => (list.length > 0 ? list.join(', ') : '—');
</script>

<section class="auths" aria-labelledby="auths-title">
  <h2 id="auths-title" class="st-title3">Auth</h2>
  <p class="intro">
    Testing an auth obtains its credentials under <strong>{profile || 'the default profile'}</strong
    >
    through the token cache, without sending a call. Only what they are called and how long they hold
    show here, never a value.
  </p>
  {#if auth.error}<p class="error" role="alert">{auth.error}</p>{/if}
  {#if auth.auths.length === 0}
    <p class="empty">The config defines no auth.</p>
  {:else}
    <table>
      <caption class="vc-sr">Auths</caption>
      <thead>
        <tr>
          <th scope="col">Auth</th>
          <th scope="col">Result</th>
          <th scope="col"><span class="vc-sr">Test</span></th>
        </tr>
      </thead>
      <tbody>
        {#each auth.auths as id (id)}
          {@const result = auth.results[id]}
          <tr>
            <th scope="row" class="vc-mono">{id}</th>
            <td>
              {#if !result}
                <span class="idle">Not tested</span>
              {:else if result.error}
                <p class="error">{result.error}</p>
              {:else}
                <dl>
                  <dt>Api</dt>
                  <dd class="vc-mono">{result.api}</dd>
                  <dt>Headers</dt>
                  <dd class="vc-mono">{names(result.headers)}</dd>
                  <dt>Query</dt>
                  <dd class="vc-mono">{names(result.query)}</dd>
                  <dt>Cookies</dt>
                  <dd class="vc-mono">{names(result.cookies)}</dd>
                  <dt>Expires in</dt>
                  <dd>
                    {result.expiresInMs === undefined ? 'not given' : lifetime(result.expiresInMs)}
                  </dd>
                </dl>
              {/if}
              {#if result}<p class="note">Under {result.profile || 'the default profile'}.</p>{/if}
            </td>
            <td class="act">
              <button
                type="button"
                class="st-btn st-btn--sm"
                aria-label="Test {id}"
                disabled={auth.testing !== undefined}
                onclick={() => void auth.test(id, profile)}
                >{auth.testing === id ? 'Testing…' : 'Test'}</button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</section>

<style>
  .auths {
    display: grid;
    gap: 12px;
  }
  .intro,
  .empty,
  .idle,
  .note {
    margin: 0;
    color: var(--st-text-secondary);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--st-chrome-separator);
    text-align: left;
    vertical-align: top;
  }
  thead th {
    color: var(--st-text-secondary);
    font-weight: 500;
  }
  tbody th {
    font-weight: 400;
  }
  .act {
    width: 1%;
    white-space: nowrap;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px 12px;
    margin: 0;
  }
  dt {
    color: var(--st-text-secondary);
  }
  dd {
    margin: 0;
  }
  .note {
    font-size: 13px;
  }
  .error {
    margin: 0;
    color: var(--vc-fail);
  }
</style>

<script lang="ts">
  import '#lib/styles/tokens.css';
  import '#lib/styles/shell.css';
  import '#lib/styles/controls.css';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { engine } from '#lib/api/transport.ts';
  import { isCurrent, liveCount, navItems } from '#lib/nav.ts';
  import { setRunSession } from '#lib/run/context.ts';
  import { RunSession } from '#lib/run/session.svelte.ts';
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();
  let collapsed = $state(false);
  let sideLabel = $derived(collapsed ? 'Expand sidebar' : 'Collapse sidebar');
  // One run at a time for the whole app, so a run started on any screen shows on Runs.
  const session = new RunSession(engine);
  setRunSession(session);
</script>

<div class="app" class:side-collapsed={collapsed}>
  <header class="topbar">
    <a class="brand" href={resolve('/tests')}>
      <svg width="22" height="22" viewBox="0 0 28 28" aria-hidden="true">
        <path
          d="M14 2 25 8.5v11L14 26 3 19.5v-11Z"
          fill="none"
          stroke="var(--accent)"
          stroke-width="2"
        />
        <path
          d="M9 10.5 14 18l5-7.5"
          fill="none"
          stroke="var(--ink)"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
      Vibranium
    </a>
  </header>
  <div class="frame">
    <nav class="side" aria-label="Main">
      <div class="side-head">
        <button
          class="iconbtn"
          type="button"
          aria-label={sideLabel}
          title={sideLabel}
          aria-expanded={!collapsed}
          onclick={() => (collapsed = !collapsed)}
        >
          <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <rect x="2" y="2.5" width="12" height="11" rx="2" />
            <path d="M6 2.5v11" />
            <path d={collapsed ? 'M9 6l2 2-2 2' : 'M11 6 9 8l2 2'} />
          </svg>
        </button>
      </div>
      {#each navItems as item (item.route)}
        {@const current = isCurrent(page.url.pathname, item.route)}
        {@const live = liveCount(item, session.running ? 1 : 0)}
        <a
          class="navitem"
          class:on={current}
          href={resolve(item.route)}
          title={item.label}
          aria-current={current ? 'page' : undefined}
        >
          <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            {#each item.icon as d (d)}
              <path {d} />
            {/each}
          </svg>
          <span class="label">{item.label}</span>
          {#if live}<span class="live"><span class="text">{live}</span></span>{/if}
        </a>
      {/each}
    </nav>
    <main class="panel">
      {@render children()}
    </main>
  </div>
</div>

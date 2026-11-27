<script lang="ts">
  /** An http method as a coloured tag: GET, POST, PUT and DELETE each have their own colour. */
  let { method }: { method: string } = $props();
</script>

<span class="method m-{method.toLowerCase()}">{method}</span>

<style>
  .method {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    min-width: 44px;
    height: 18px;
    padding: 0 6px;
    border-radius: var(--st-radius-capsule);
    font-size: 10.5px;
    font-weight: 600;
    background: var(--st-accent-soft);
    color: var(--st-accent);
  }
  .m-get {
    background: var(--vc-pass-soft);
    color: var(--vc-pass);
  }
  .m-post {
    background: var(--vc-flaky-soft);
    color: var(--vc-flaky);
  }
  .m-put {
    background: var(--vc-blocked-soft);
    color: var(--vc-blocked);
  }
  .m-delete {
    background: var(--vc-fail-soft);
    color: var(--vc-fail);
  }
</style>

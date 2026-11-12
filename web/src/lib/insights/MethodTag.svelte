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
    border-radius: 5px;
    font-size: 10.5px;
    font-weight: 600;
    background: var(--accentSoft);
    color: var(--accentInk);
  }
  .m-get {
    background: var(--passSoft);
    color: var(--pass);
  }
  .m-post {
    background: var(--flakySoft);
    color: var(--flaky);
  }
  .m-put {
    background: var(--blockSoft);
    color: var(--block);
  }
  .m-delete {
    background: var(--failSoft);
    color: var(--fail);
  }
</style>

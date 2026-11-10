<script lang="ts">
  import { page } from '$app/state';
  import { engine } from '#lib/api/transport.ts';
  import Editor from '#lib/editor/Editor.svelte';
  import { EditorSession } from '#lib/editor/session.svelte.ts';
  import { getRunSession } from '#lib/run/context.ts';
  import Screen from '#lib/Screen.svelte';

  const session = getRunSession();
  const editor = new EditorSession(engine);
  void editor.loadFiles();
  const file = page.url.searchParams.get('file');
  if (file) void editor.open(file);
</script>

<Screen title="Editor">
  <Editor {editor} {session} />
</Screen>

<script lang="ts">
  import { untrack } from 'svelte';
  import { page } from '$app/state';
  import { engine } from '#lib/api/transport.ts';
  import Editor from '#lib/editor/Editor.svelte';
  import { engineEditorSeams } from '#lib/editor/port.ts';
  import { EditorSession } from '#lib/editor/session.svelte.ts';
  import { getRunSession } from '#lib/run/context.ts';
  import Screen from '#lib/Screen.svelte';
  import { getVcsSession } from '#lib/vcs/context.ts';

  const session = getRunSession();
  const editor = new EditorSession(engine);
  const seams = engineEditorSeams(engine);
  void editor.loadFiles();
  const file = page.url.searchParams.get('file');
  if (file) void editor.open(file);
  const vcs = getVcsSession();
  // A pull changed the files on disk: list them again, and reload the open one unless it has edits.
  $effect(() => {
    if (vcs.revision === 0) return;
    untrack(() => {
      void editor.loadFiles();
      if (editor.buffer && !editor.dirty) void editor.reload();
    });
  });
</script>

<Screen title="Editor">
  <Editor {editor} {session} {seams} />
</Screen>

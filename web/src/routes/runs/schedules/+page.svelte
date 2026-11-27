<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import { engineSchedulesPort } from '#lib/schedules/port.ts';
  import { ScheduleList } from '#lib/schedules/schedules.svelte.ts';
  import SchedulesScreen from '#lib/schedules/SchedulesScreen.svelte';
  import Screen from '#lib/Screen.svelte';
  import { getProjectStatus } from '#lib/shell/project.svelte.ts';
  import { getToasts } from '#lib/ui/toasts.svelte.ts';

  const project = getProjectStatus();
  const list = new ScheduleList(engineSchedulesPort(engine), getToasts());
  void list.load();
</script>

<Screen title="Schedules">
  <SchedulesScreen {list} profiles={project.profiles} />
</Screen>

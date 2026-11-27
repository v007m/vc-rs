import type { ScheduleSpec } from './port.ts';

/** The schedule dialog's fields: the selector as typed, the rest as the spec has them. */
export type ScheduleForm = {
  id: string;
  name: string;
  cron: string;
  selector: string;
  profiles: readonly string[];
  runner: string;
  enabled: boolean;
};

/** A new schedule: every day at 02:00 UTC on the server, on. */
export const emptyForm: ScheduleForm = {
  id: '',
  name: '',
  cron: '0 2 * * *',
  selector: '',
  profiles: [],
  runner: 'vcServer',
  enabled: true,
};

/** The form for editing `spec`. */
export function formOf(spec: ScheduleSpec): ScheduleForm {
  const { id, name = '', cron, selector, profiles, runner, enabled } = spec;
  return { id, name, cron, selector: selector.join(' '), profiles, runner, enabled };
}

const CAMEL = /^[a-z][a-zA-Z0-9]*$/;

/** Why `id` cannot name a new schedule, or `undefined` when it can (D700: camelCase, unique). */
export function idProblem(id: string, taken: readonly string[]): string | undefined {
  if (!id) return 'Give the schedule an ID.';
  if (!CAMEL.test(id)) return 'Use camelCase: a lowercase letter, then letters and digits.';
  return taken.includes(id) ? `A schedule named ${id} exists already.` : undefined;
}

/** The spec the form writes; `notify` is kept from the schedule it edits, as the form has none. */
export function specFrom(form: ScheduleForm, notify: ScheduleSpec['notify'] = []): ScheduleSpec {
  const name = form.name.trim();
  return {
    id: form.id.trim(),
    ...(name ? { name } : {}),
    cron: form.cron.trim().replace(/\s+/g, ' '),
    selector: form.selector.split(/\s+/).filter(Boolean),
    profiles: form.profiles,
    notify,
    runner: form.runner,
    enabled: form.enabled,
  };
}

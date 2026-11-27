/** The days of the week by cron number; 7 is Sunday, as 0 is (D700). */
const DAYS = [
  'Sunday',
  'Monday',
  'Tuesday',
  'Wednesday',
  'Thursday',
  'Friday',
  'Saturday',
  'Sunday',
] as const;

const NUMBER = /^\d{1,2}$/;

/** `n` when `field` is one whole number within `max`. */
function single(field: string, max: number): number | undefined {
  if (!NUMBER.test(field)) return undefined;
  const n = Number(field);
  return n <= max ? n : undefined;
}

/** `n` when `field` is a step `*\/n` of at least 2. */
function step(field: string): number | undefined {
  const n = /^\*\/(\d{1,2})$/.exec(field)?.[1];
  return n !== undefined && Number(n) > 1 ? Number(n) : undefined;
}

const two = (n: number) => String(n).padStart(2, '0');

/** `02:00 UTC`: the clock time a minute and hour field name. */
function clock(minute: string, hour: string): string | undefined {
  const m = single(minute, 59);
  const h = single(hour, 23);
  return m === undefined || h === undefined ? undefined : `${two(h)}:${two(m)} UTC`;
}

/** `Every day`, `Weekdays`, `Every Monday and Thursday`, `Monday to Wednesday` for a day field. */
function days(field: string): string | undefined {
  if (field === '*') return 'Every day';
  if (field === '1-5') return 'Weekdays';
  if (field === '0,6' || field === '6,0') return 'Weekends';
  const range = /^([0-7])-([0-7])$/.exec(field);
  if (range) return `${DAYS[Number(range[1])]} to ${DAYS[Number(range[2])]}`;
  const listed = field.split(',').map((day) => single(day, 7));
  if (listed.some((day) => day === undefined)) return undefined;
  return `Every ${listed.map((day) => DAYS[day ?? 0]).join(' and ')}`;
}

/** The forms that fire at least hourly: every minute, every n minutes, hourly at a minute. */
function frequent(minute: string, hour: string): string | undefined {
  if (hour !== '*') {
    const hours = step(hour);
    const m = single(minute, 59);
    return hours && m !== undefined ? `Every ${hours} hours at :${two(m)}` : undefined;
  }
  if (minute === '*') return 'Every minute';
  const minutes = step(minute);
  if (minutes) return `Every ${minutes} minutes`;
  const m = single(minute, 59);
  return m === undefined ? undefined : `Every hour at :${two(m)}`;
}

/** The forms at a clock time: on some days of the week, or on one day of each month. */
function daily(minute: string, hour: string, date: string, weekday: string): string | undefined {
  const at = clock(minute, hour);
  if (!at) return undefined;
  if (date === '*') {
    const on = days(weekday);
    return on && `${on} at ${at}`;
  }
  const day = single(date, 31);
  return day && weekday === '*' ? `Day ${day} of each month at ${at}` : undefined;
}

/**
 * A five-field UTC cron in words for its common forms (`0 2 * * *` is "Every day at 02:00
 * UTC", `*\/15 * * * *` "Every 15 minutes"); `undefined` for any other, which shows as written.
 */
export function cronWords(cron: string): string | undefined {
  const fields = cron.trim().split(/\s+/);
  if (fields.length !== 5) return undefined;
  const [minute = '', hour = '', date = '', month = '', weekday = ''] = fields;
  if (month !== '*') return undefined;
  if (date === '*' && weekday === '*') {
    const often = frequent(minute, hour);
    if (often) return often;
  }
  return daily(minute, hour, date, weekday);
}

/** `in 5 min`, `in 3 h`, `in 2 d`, or the date: when a schedule next fires, short. */
export function untilWords(atMs: number, now = Date.now()): string {
  const minutes = Math.round((atMs - now) / 60_000);
  if (minutes < 1) return 'now';
  if (minutes < 60) return `in ${minutes} min`;
  if (minutes < 60 * 24) return `in ${Math.round(minutes / 60)} h`;
  if (minutes < 60 * 24 * 7) return `in ${Math.round(minutes / 1440)} d`;
  return new Date(atMs).toLocaleDateString();
}

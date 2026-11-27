import type { ExchangeLine } from '#lib/results/types.ts';
import { adhocWords, type StreamCall, type StreamEvent, type StreamPort } from './seams.ts';
import { message } from './tab.svelte.ts';

export type StreamStatus = 'idle' | 'connecting' | 'open' | 'ended';

export type Direction = 'in' | 'out';

/** One message on a stream, numbered from 1 in the order it came. */
export type StreamFrame = Readonly<{
  n: number;
  direction: Direction;
  atMs: number;
  json: string;
  size: number;
  /** `json` in lower case, made once, for the filter (D756). */
  search: string;
}>;

export type StreamEnd = Readonly<{ reason: string; failed: boolean; code?: number }>;

/** The most messages a stream view keeps; older ones drop off the top (D564). */
export const frameLimit = 1000;

const encoder = new TextEncoder();

/** Runs `flush` once, later: before the next paint, or in a microtask where nothing paints. */
export type Schedule = (flush: () => void) => void;

const nextPaint: Schedule = (flush) => {
  if (typeof requestAnimationFrame === 'function') requestAnimationFrame(() => flush());
  else queueMicrotask(flush);
};

/** `kept` then `incoming`, newest last, cut to the limit without copying a long burst whole. */
function joined(kept: readonly StreamFrame[], incoming: readonly StreamFrame[]): StreamFrame[] {
  const tail = incoming.length > frameLimit ? incoming.slice(-frameLimit) : incoming;
  return kept.concat(tail).slice(-frameLimit);
}

/**
 * One open SSE or WebSocket stream (D564): its messages as they come, how it ended, and what
 * the view shows of them. Pausing holds new messages back until resumed; the stream keeps
 * reading. Closing asks the engine to close it; aborting stops reading at once. Messages are
 * batched and shown once per paint, so a burst replaces the list once, not once per message (D756).
 */
export class StreamSession {
  status: StreamStatus = $state('idle');
  frames: readonly StreamFrame[] = $state.raw([]);
  /** Messages that came while paused, shown on resume. */
  held: readonly StreamFrame[] = $state.raw([]);
  paused = $state(false);
  head: ExchangeLine | undefined = $state();
  runId: number | undefined = $state();
  ended: StreamEnd | undefined = $state();
  error: string | undefined = $state();
  #stream: string | undefined;
  #abort: AbortController | undefined;
  #next = 1;
  /** Messages taken since the last flush. */
  #incoming: StreamFrame[] = [];
  #due = false;
  readonly #port: StreamPort;
  readonly #schedule: Schedule;

  constructor(port: StreamPort, schedule: Schedule = nextPaint) {
    this.#port = port;
    this.#schedule = schedule;
  }

  get live(): boolean {
    return this.status === 'connecting' || this.status === 'open';
  }

  /** Opens the stream and reads it until it ends, the engine fails, or it is aborted. */
  async connect(call: StreamCall): Promise<void> {
    if (this.live) return;
    this.#reset();
    const abort = new AbortController();
    this.#abort = abort;
    this.status = 'connecting';
    try {
      for await (const event of this.#port.openStream(call, abort.signal)) this.#take(event);
    } catch (error) {
      if (!abort.signal.aborted) this.error = adhocWords(message(error));
    } finally {
      if (this.#abort === abort) this.#abort = undefined;
      this.#flush();
      this.status = 'ended';
    }
  }

  /** Sends one text message on an open WebSocket. */
  async send(text: string): Promise<void> {
    if (this.status !== 'open' || this.#stream === undefined) return;
    try {
      await this.#port.streamSend(this.#stream, text);
      this.error = undefined;
    } catch (error) {
      this.error = message(error);
    }
  }

  /** Asks the engine to close the stream; when it cannot, stops reading. */
  async close(): Promise<void> {
    if (!this.live) return;
    if (this.#stream === undefined) {
      this.abort();
      return;
    }
    try {
      await this.#port.streamSend(this.#stream, undefined);
    } catch {
      this.abort();
    }
  }

  abort(): void {
    this.#abort?.abort();
  }

  setPaused(paused: boolean): void {
    this.#flush();
    this.paused = paused;
    if (paused) return;
    this.frames = joined(this.frames, this.held);
    this.held = [];
  }

  clear(): void {
    this.frames = [];
    this.held = [];
    this.#incoming = [];
  }

  #reset(): void {
    this.clear();
    this.head = undefined;
    this.runId = undefined;
    this.ended = undefined;
    this.error = undefined;
    this.#stream = undefined;
    this.#next = 1;
  }

  #take(event: StreamEvent): void {
    if (event.kind === 'opened') {
      this.#stream = event.stream;
      this.runId = event.runId;
      this.head = event.head;
      this.status = 'open';
      return;
    }
    if (event.kind === 'ended') {
      const { failed, code } = event;
      const reason = adhocWords(event.reason);
      this.ended = code === undefined ? { reason, failed } : { reason, failed, code };
      return;
    }
    const { direction, atMs, json } = event;
    const size = encoder.encode(json).length;
    this.#incoming.push({
      n: this.#next++,
      direction,
      atMs,
      json,
      size,
      search: json.toLowerCase(),
    });
    if (this.#due) return;
    this.#due = true;
    this.#schedule(() => {
      this.#due = false;
      this.#flush();
    });
  }

  /** Shows what came since the last flush, or holds it while paused. */
  #flush(): void {
    const incoming = this.#incoming;
    if (incoming.length === 0) return;
    this.#incoming = [];
    if (this.paused) this.held = joined(this.held, incoming);
    else this.frames = joined(this.frames, incoming);
  }
}

export type FrameView = Readonly<{ direction: Direction | 'all'; filter: string }>;

/** The frames a view shows: in one direction or both, and holding `filter` in their body. */
export function shownFrames(frames: readonly StreamFrame[], view: FrameView): StreamFrame[] {
  const text = view.filter.trim().toLowerCase();
  return frames.filter(
    (f) =>
      (view.direction === 'all' || f.direction === view.direction) &&
      (text === '' || f.search.includes(text)),
  );
}

function fields(json: string): Record<string, unknown> {
  try {
    const value: unknown = JSON.parse(json);
    return typeof value === 'object' && value !== null && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
}

/** What a frame carried: its SSE `data` or WebSocket `text`, of any JSON type, if it has one. */
function payload(json: string): { value: unknown } | undefined {
  const envelope = fields(json);
  if ('data' in envelope) return { value: envelope.data };
  if ('text' in envelope) return { value: envelope.text };
  return undefined;
}

/** A frame's kind as the list shows it: an SSE event's name, else `text` or `json`. */
export function frameKind(json: string): string {
  const { event } = fields(json);
  if (typeof event === 'string' && event !== '') return event;
  const carried = payload(json)?.value;
  if (typeof carried !== 'string') return 'json';
  return /^\s*[[{]/.test(carried) && parses(carried) ? 'json' : 'text';
}

/**
 * What a frame carried, pretty when it is JSON (D754). The engine sends SSE `data` already parsed,
 * so it may be any JSON value; only a frame with neither `data` nor `text` shows its envelope.
 */
export function frameBody(json: string): string {
  const carried = payload(json);
  if (carried === undefined) return pretty(json);
  const { value } = carried;
  return typeof value === 'string' ? pretty(value) : JSON.stringify(value, null, 2);
}

function parses(text: string): boolean {
  try {
    JSON.parse(text);
    return true;
  } catch {
    return false;
  }
}

function pretty(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

const timeOfDay = new Intl.DateTimeFormat('en-GB', {
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  fractionalSecondDigits: 3,
  hour12: false,
});

/** When a frame came: a time of day with milliseconds, or an offset when the stamp is relative. */
export function frameTime(atMs: number): string {
  return atMs < 1e11 ? `+${atMs} ms` : timeOfDay.format(atMs);
}

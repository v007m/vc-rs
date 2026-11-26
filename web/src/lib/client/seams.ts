import type { Client } from '@connectrpc/connect';
import type { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { outcomes, type RunOutcome } from '#lib/history/port.ts';
import type { CallOutput } from './port.ts';

/**
 * The Client's seams for the RPCs the engine is still adding (D538): ad hoc calls, streams and
 * call history. A method the engine cannot serve yet answers `Unavailable` with the reason, and
 * the screen shows a quiet empty state instead of an error.
 */
export type Unavailable = Readonly<{ available: false; reason: string }>;

export type Available<T> = Readonly<{ available: true; value: T }>;

export type Maybe<T> = Available<T> | Unavailable;

export function notYet(reason: string): Unavailable {
  return { available: false, reason };
}

export function available<T>(value: T): Available<T> {
  return { available: true, value };
}

/** A request with no api behind it: a scratch tab's method, URL, headers, body and auth. */
export type AdhocRequest = Readonly<{
  method: string;
  url: string;
  headers: readonly (readonly [name: string, value: string])[];
  body?: string;
  /** An auth from the project's registry, by its id. */
  auth?: string;
  vars: readonly string[];
}>;

export interface AdhocPort {
  callAdhoc(request: AdhocRequest): Promise<Maybe<CallOutput>>;
}

/** One message of an SSE or WebSocket stream, as the stream list shows it. */
export type StreamFrame = Readonly<{
  atMs: number;
  direction: 'in' | 'out';
  kind: 'text' | 'json' | 'binary' | 'event';
  data: string;
  size: number;
}>;

/** An open stream: frames arrive through the callback given to `connect`. */
export interface StreamSession {
  send(data: string): Promise<Maybe<void>>;
  close(): void;
}

export interface StreamPort {
  connect(
    api: string,
    vars: readonly string[],
    onframe: (frame: StreamFrame) => void,
  ): Promise<Maybe<StreamSession>>;
}

/** One stored call in the history: what it called, how it went and when. */
export type HistoryEntry = Readonly<{
  id: number;
  method?: string;
  /** The URL path, or the api's full ID when the call named one. */
  label: string;
  api?: string;
  outcome: RunOutcome;
  startedMs: number;
}>;

export interface HistorySource {
  recent(limit: number): Promise<Maybe<HistoryEntry[]>>;
}

export type ClientSeams = AdhocPort & StreamPort & HistorySource;

/** An ad hoc run's selector: `GET /users/1` for a call with no api, or the api's full ID. */
export function historyEntry(
  run: Readonly<{ id: number; selector: string; outcome: RunOutcome; startedMs: number }>,
): HistoryEntry {
  const { id, selector, outcome, startedMs } = run;
  const sent = /^([A-Z]+)\s+(\S+)$/.exec(selector);
  if (sent?.[1] && sent[2]) return { id, method: sent[1], label: sent[2], outcome, startedMs };
  return { id, label: selector, api: selector, outcome, startedMs };
}

export const adhocNotYet = 'Calls without an api need an engine update. Not available yet.';
export const streamNotYet = 'Streams need the engine’s CallStream. Not available yet.';

/** The seams over the engine as it is: history from ListRuns, the rest not available yet. */
export function engineSeams(client: Pick<Client<typeof EngineService>, 'listRuns'>): ClientSeams {
  return {
    callAdhoc: () => Promise.resolve(notYet(adhocNotYet)),
    connect: () => Promise.resolve(notYet(streamNotYet)),
    async recent(limit) {
      const { runs } = await client.listRuns({ limit, kind: 'adhoc' });
      return available(
        runs.map((run) =>
          historyEntry({
            id: Number(run.id),
            selector: run.selector,
            outcome: outcomes[run.status] ?? 'unfinished',
            startedMs: Number(run.startedMs),
          }),
        ),
      );
    },
  };
}

/** Seams that serve nothing, for screens and tests that do not use them. */
export const noSeams: ClientSeams = {
  callAdhoc: () => Promise.resolve(notYet(adhocNotYet)),
  connect: () => Promise.resolve(notYet(streamNotYet)),
  recent: () => Promise.resolve(available([])),
};

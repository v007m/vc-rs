import type { MessageInitShape } from '@bufbuild/protobuf';
import { Code, ConnectError, type Client } from '@connectrpc/connect';
import type {
  CallRequestSchema,
  CallStreamResponse,
  EngineService,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import type { Run } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { exchangeLine } from '#lib/api/results.ts';
import { outcomes, type RunOutcome } from '#lib/history/port.ts';
import type { ExchangeLine } from '#lib/results/types.ts';
import { callOutput, type CallOutput } from './port.ts';

/**
 * The Client's seams over the RPCs added for it (D538): ad hoc calls, streams and call
 * history. An engine without one answers `unimplemented`; the seam turns that into
 * `Unavailable` with the reason, and the screen shows a quiet empty state, not an error.
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

export type Pair = readonly [name: string, value: string];

export type CallType = 'http' | 'sse' | 'websocket';

/** A request with no api behind it: a scratch tab's request (D540, D541). */
export type AdhocRequest = Readonly<{
  type: CallType;
  method: string;
  url: string;
  headers: readonly Pair[];
  query: readonly Pair[];
  body?: string;
  /** An auth from the project's config, by its ID. */
  auth?: string;
  vars: readonly string[];
}>;

/** The file the engine names in a diagnostic about an ad hoc request. */
export const adhocFile = 'services/vc-adhoc/vc-adhoc.api.yaml';

export interface AdhocPort {
  callAdhoc(request: AdhocRequest): Promise<Maybe<CallOutput>>;
}

/** What a stream opens: an api with its call vars, or an ad hoc request. */
export type StreamCall = Readonly<{ api: string; vars: readonly string[] }> | AdhocRequest;

/** What a stream reports, in order: it opened, then its messages, then how it ended. */
export type StreamEvent =
  | Readonly<{ kind: 'opened'; stream: string; runId: number; head?: ExchangeLine }>
  | Readonly<{ kind: 'message'; direction: 'in' | 'out'; atMs: number; json: string }>
  | Readonly<{ kind: 'ended'; reason: string; failed: boolean; code?: number }>;

export interface StreamPort {
  /** The stream's events until it ends; aborting `signal` closes it. */
  openStream(call: StreamCall, signal: AbortSignal): AsyncIterable<StreamEvent>;
  /** Sends one WebSocket text message, or closes the stream when `text` is absent. */
  streamSend(stream: string, text?: string): Promise<void>;
}

/** One stored call in the history: what it called, how it went and when. */
export type HistoryEntry = Readonly<{
  id: number;
  method?: string;
  /** The URL as sent, masked, or the run's selector when it has no call summary. */
  label: string;
  status?: number;
  outcome: RunOutcome;
  startedMs: number;
}>;

export interface HistorySource {
  recent(limit: number): Promise<Maybe<HistoryEntry[]>>;
}

export type ClientSeams = AdhocPort & StreamPort & HistorySource;

export const adhocNotYet = 'Sending a request with no api needs a newer engine.';
export const streamNotYet = 'Streams need a newer engine.';
export const historyNotYet = 'Call history needs a newer engine.';

/** What a history line needs of a run. */
export type HistoryRun = Pick<Run, 'id' | 'selector' | 'status' | 'startedMs'> & {
  call?: Readonly<{ method: string; url: string; status?: number }> | undefined;
};

/** A run of kind `adhoc` as a history line (D542). */
export function historyEntry(run: HistoryRun): HistoryEntry {
  const at = {
    id: Number(run.id),
    outcome: outcomes[run.status] ?? 'unfinished',
    startedMs: Number(run.startedMs),
  };
  const call = run.call;
  if (!call) return { ...at, label: run.selector };
  const status = call.status === undefined ? {} : { status: call.status };
  return { ...at, method: call.method, label: call.url, ...status };
}

function pairs(list: readonly Pair[]): { name: string; value: string }[] {
  return list.map(([name, value]) => ({ name, value }));
}

/** `call` as the engine's request: an api by ID, or the ad hoc request in its place. */
export function callRequest(call: StreamCall): MessageInitShape<typeof CallRequestSchema> {
  if ('api' in call) return { api: call.api, vars: [...call.vars] };
  const { type, method, url, headers, query, body, auth, vars } = call;
  const adhoc = { type: type === 'http' ? '' : type, method, url, body, auth };
  const lists = { headers: pairs(headers), query: pairs(query) };
  return { vars: [...vars], adhoc: { ...adhoc, ...lists } };
}

/** The value of `work`, or `Unavailable` when the engine has no such RPC. */
async function unlessMissing<T>(reason: string, work: () => Promise<T>): Promise<Maybe<T>> {
  try {
    return available(await work());
  } catch (error) {
    if (ConnectError.from(error).code === Code.Unimplemented) return notYet(reason);
    throw error;
  }
}

function streamEvent({ event }: CallStreamResponse): StreamEvent | undefined {
  switch (event.case) {
    case 'opened': {
      const { stream, runId, head } = event.value;
      const opened = { kind: 'opened' as const, stream, runId: Number(runId) };
      return head ? { ...opened, head: exchangeLine(head) } : opened;
    }
    case 'message': {
      const { direction, atMs, json } = event.value;
      const dir = direction === 'out' ? 'out' : 'in';
      return { kind: 'message', direction: dir, atMs: Number(atMs), json };
    }
    case 'ended': {
      const { reason, failed, code } = event.value;
      const ended = { kind: 'ended' as const, reason, failed };
      return code === undefined ? ended : { ...ended, code };
    }
    default:
      return undefined;
  }
}

type SeamRpcs = 'call' | 'callStream' | 'streamSend' | 'listRuns';

/** The seams over the engine's Call, CallStream, StreamSend and ListRuns RPCs. */
export function engineSeams(client: Pick<Client<typeof EngineService>, SeamRpcs>): ClientSeams {
  return {
    callAdhoc: (request) =>
      unlessMissing(adhocNotYet, async () => callOutput(await client.call(callRequest(request)))),
    async *openStream(call, signal) {
      for await (const answer of client.callStream({ call: callRequest(call) }, { signal })) {
        const event = streamEvent(answer);
        if (event) yield event;
      }
    },
    async streamSend(stream, text) {
      await client.streamSend(text === undefined ? { stream, close: true } : { stream, text });
    },
    recent: (limit) =>
      unlessMissing(historyNotYet, async () => {
        const { runs } = await client.listRuns({ limit, kind: 'adhoc' });
        return runs.map(historyEntry);
      }),
  };
}

function missing(reason: string): AsyncIterable<never> {
  const next = () => Promise.reject(new ConnectError(reason, Code.Unimplemented));
  return { [Symbol.asyncIterator]: () => ({ next }) };
}

/** Seams over an engine with none of these RPCs, for screens and tests that do not use them. */
export const noSeams: ClientSeams = {
  callAdhoc: () => Promise.resolve(notYet(adhocNotYet)),
  openStream: () => missing(streamNotYet),
  streamSend: () => Promise.reject(new ConnectError(streamNotYet, Code.Unimplemented)),
  recent: () => Promise.resolve(notYet(historyNotYet)),
};

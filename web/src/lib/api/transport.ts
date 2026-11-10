import { createClient, type Client } from '@connectrpc/connect';
import { createConnectTransport } from '@connectrpc/connect-web';
import { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/**
 * The Connect transport to the engine API that `vc ui` serves at `/api`. The wire is binary
 * protobuf only. Auth is the same-origin `vc_token_<port>` session cookie the server sets, so no
 * token is handled here and credentials never go to another origin.
 */
export const transport = createConnectTransport({
  baseUrl: '/api',
  useBinaryFormat: true,
  fetch: (input, init) => globalThis.fetch(input, { ...init, credentials: 'same-origin' }),
});

/** The engine of the project `vc ui` opened. */
export const engine: Client<typeof EngineService> = createClient(EngineService, transport);

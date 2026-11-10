import { create, type MessageInitShape } from '@bufbuild/protobuf';
import {
  createClient,
  createRouterTransport,
  type Client,
  type ServiceImpl,
} from '@connectrpc/connect';
import { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { RunEventSchema, type RunEvent } from '#lib/gen/vibranium/engine/v1/events_pb.ts';

/** An engine client served in memory by `impl`; unimplemented RPCs answer `unimplemented`. */
export function fakeEngine(
  impl: Partial<ServiceImpl<typeof EngineService>>,
): Client<typeof EngineService> {
  const transport = createRouterTransport(({ service }) => service(EngineService, impl));
  return createClient(EngineService, transport);
}

/** Run events numbered from 1, built from their `kind` alone. */
export function events(...kinds: MessageInitShape<typeof RunEventSchema>['kind'][]): RunEvent[] {
  return kinds.map((kind, i) => create(RunEventSchema, { seq: BigInt(i + 1), kind }));
}

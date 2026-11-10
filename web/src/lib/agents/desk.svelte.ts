import { ConnectError, type Client } from '@connectrpc/connect';
import {
  type AgentAction,
  type Approval,
  type EngineService,
  type Verdict,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** The part of the engine client the Agents screen needs, so tests can pass a fake. */
export type DeskClient = Pick<
  Client<typeof EngineService>,
  'listApprovals' | 'decideApproval' | 'listAgentActions'
>;

/** How many log entries the screen loads. */
const logLimit = 200;

/** What agents are waiting on and what they did, refreshed from the engine. */
export class AgentDesk {
  pending: readonly Approval[] = $state([]);
  actions: readonly AgentAction[] = $state([]);
  error: string | undefined = $state();
  /** The approval being decided, so its buttons stay disabled until the engine answers. */
  deciding: bigint | undefined = $state();
  readonly #client: DeskClient;

  constructor(client: DeskClient) {
    this.#client = client;
  }

  /** Reloads the pending approvals and the activity log. */
  async refresh(): Promise<void> {
    try {
      const [approvals, actions] = await Promise.all([
        this.#client.listApprovals({ pendingOnly: true }),
        this.#client.listAgentActions({ limit: logLimit }),
      ]);
      this.pending = approvals.approvals;
      this.actions = actions.actions;
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    }
  }

  /** Sends a person's verdict on approval `id`, then refreshes. */
  async decide(id: bigint, verdict: Verdict): Promise<void> {
    this.deciding = id;
    try {
      await this.#client.decideApproval({ id, verdict });
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.deciding = undefined;
    }
    await this.refresh();
  }

  /** Refreshes now and every `ms`; the returned function stops it. */
  poll(ms: number): () => void {
    void this.refresh();
    const timer = setInterval(() => void this.refresh(), ms);
    return () => clearInterval(timer);
  }
}

/** How long ago `ms` (Unix ms) was, in words, as seen at `now`. */
export function ago(ms: bigint, now: number): string {
  const seconds = Math.max(0, Math.round((now - Number(ms)) / 1000));
  if (seconds < 60) return `${seconds} s ago`;
  const minutes = Math.round(seconds / 60);
  return minutes < 60 ? `${minutes} min ago` : `${Math.round(minutes / 60)} h ago`;
}

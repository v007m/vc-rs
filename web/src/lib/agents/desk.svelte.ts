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
  'listApprovals' | 'decideApproval' | 'listAgentActions' | 'connection'
>;

/** Where an MCP client reaches this vc ui and the agent token it sends; held in memory only. */
export type McpLink = Readonly<{ url: string; token: string }>;

/** How many log entries the screen loads. */
const logLimit = 200;

/** How often a failed load of the MCP endpoint and agent token is tried again. */
export const linkRetryMs = 60_000;

/** What agents are waiting on and what they did, refreshed from the engine. */
export class AgentDesk {
  pending: readonly Approval[] = $state([]);
  actions: readonly AgentAction[] = $state([]);
  error: string | undefined = $state();
  /** The approval being decided, so its buttons stay disabled until the engine answers. */
  deciding: bigint | undefined = $state();
  link: McpLink | undefined = $state.raw();
  linkError: string | undefined = $state();
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

  /** Sends a person's verdict on approval `id`, then refreshes; a failed verdict stays shown. */
  async decide(id: bigint, verdict: Verdict): Promise<void> {
    this.deciding = id;
    let failure: string | undefined;
    try {
      await this.#client.decideApproval({ id, verdict });
    } catch (error) {
      failure = ConnectError.from(error).rawMessage;
    } finally {
      this.deciding = undefined;
    }
    await this.refresh();
    if (failure) this.error = failure;
  }

  /** Loads the MCP endpoint and agent token once, since they hold while vc ui runs. */
  async connect(): Promise<void> {
    if (this.link) return;
    try {
      const { mcpUrl, agentToken } = await this.#client.connection({});
      this.link = { url: mcpUrl, token: agentToken };
      this.linkError = undefined;
    } catch (error) {
      this.linkError = ConnectError.from(error).rawMessage;
    }
  }

  /**
   * Loads the MCP endpoint, trying again every `linkRetryMs` while that fails, and refreshes now
   * and every `ms`; the returned function stops both.
   */
  poll(ms: number): () => void {
    void this.connect();
    void this.refresh();
    const timer = setInterval(() => void this.refresh(), ms);
    const retry = setInterval(() => {
      if (this.linkError !== undefined) void this.connect();
    }, linkRetryMs);
    return () => {
      clearInterval(timer);
      clearInterval(retry);
    };
  }
}

/** How long ago `ms` (Unix ms) was, in words, as seen at `now`. */
export function ago(ms: bigint, now: number): string {
  const seconds = Math.max(0, Math.round((now - Number(ms)) / 1000));
  if (seconds < 60) return `${seconds} s ago`;
  const minutes = Math.round(seconds / 60);
  return minutes < 60 ? `${minutes} min ago` : `${Math.round(minutes / 60)} h ago`;
}

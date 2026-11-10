import { fromSetup } from './support';

/** The name the e2e agent gives in its MCP `initialize`, which the agent log shows. */
export const e2eAgent = 'vc-e2e-agent';

/** Posts one JSON-RPC message to vc ui's /mcp as an agent does, with the agent token. */
async function post(body: object, session?: string): Promise<Response> {
  const headers: Record<string, string> = {
    accept: 'application/json, text/event-stream',
    authorization: `Bearer ${fromSetup('VC_E2E_AGENT_TOKEN')}`,
    'content-type': 'application/json',
  };
  if (session) headers['mcp-session-id'] = session;
  const response = await fetch(fromSetup('VC_E2E_MCP_URL'), {
    method: 'POST',
    headers,
    body: JSON.stringify(body),
  });
  if (!response.ok) throw new Error(`/mcp answered ${response.status}: ${await response.text()}`);
  return response;
}

/** Opens an MCP session as `e2eAgent` and calls the reading tool `tool` once. */
export async function callTool(tool: string, args: object = {}): Promise<string> {
  const opened = await post({
    jsonrpc: '2.0',
    id: 1,
    method: 'initialize',
    params: {
      protocolVersion: '2025-03-26',
      capabilities: {},
      clientInfo: { name: e2eAgent, version: '0' },
    },
  });
  await opened.text();
  const session = opened.headers.get('mcp-session-id') ?? undefined;
  await (await post({ jsonrpc: '2.0', method: 'notifications/initialized' }, session)).text();
  const called = await post(
    { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: tool, arguments: args } },
    session,
  );
  return called.text();
}

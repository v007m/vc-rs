import type { ApiInfo, CallInput, ClientPort, FlowInfo } from './port.ts';
import type { ApiRequest } from './request.svelte.ts';
import { ScratchTab } from './scratch.svelte.ts';
import { noSeams, type AdhocPort, type StreamPort } from './seams.ts';
import { CallTab, message, type CallResult, type VarRow } from './tab.svelte.ts';

export type { CallResult, VarRow } from './tab.svelte.ts';

export type ClientTab = CallTab | ScratchTab;

/**
 * The Client space's state: the project's apis and flows, and the open tabs (D539). The call
 * members act on the active tab when it is an api tab, so a screen reads `client.request`,
 * `client.result` and so on for whatever tab is in front.
 */
export class ClientSession {
  apis: readonly ApiInfo[] = $state([]);
  flows: readonly FlowInfo[] = $state([]);
  tabs: ClientTab[] = $state([]);
  activeId: string | undefined = $state();
  /** How many sends this session made, so a history view knows to read again. */
  sent = $state(0);
  #error: string | undefined = $state();
  #next = 1;
  readonly #port: ClientPort;
  readonly #adhoc: AdhocPort & StreamPort;

  constructor(port: ClientPort, adhoc: AdhocPort & StreamPort = noSeams) {
    this.#port = port;
    this.#adhoc = adhoc;
  }

  get tab(): ClientTab | undefined {
    return this.tabs.find((t) => t.id === this.activeId);
  }

  /** The active tab when it is on an api. */
  get call(): CallTab | undefined {
    const tab = this.tab;
    return tab?.kind === 'api' ? tab : undefined;
  }

  get current(): ApiInfo | undefined {
    return this.call?.api;
  }

  get request(): ApiRequest | undefined {
    return this.call?.request;
  }

  get vars(): VarRow[] {
    return this.call?.vars ?? [];
  }

  get keep(): boolean {
    return this.call?.keep ?? false;
  }

  set keep(keep: boolean) {
    if (this.call) this.call.keep = keep;
  }

  get sending(): boolean {
    return this.tab?.sending ?? false;
  }

  get result(): CallResult | undefined {
    return this.call?.result;
  }

  get error(): string | undefined {
    return this.#error ?? this.tab?.error;
  }

  get input(): CallInput | undefined {
    return this.call?.input;
  }

  async load(): Promise<void> {
    try {
      [this.apis, this.flows] = await Promise.all([this.#port.apis(), this.#port.flows()]);
      this.#error = undefined;
    } catch (error) {
      this.#error = message(error);
    }
  }

  /** Brings the api's tab to the front, opening a tab for it when there is none. */
  async open(id: string): Promise<void> {
    const api = this.apis.find((a) => a.id === id);
    if (!api || api.id === this.current?.id) return;
    const open = this.tabs.find((t) => t.kind === 'api' && t.api.id === id);
    if (open) {
      this.activeId = open.id;
      return;
    }
    const tab = new CallTab(this.#port, this.#id(), api);
    this.tabs.push(tab);
    this.activeId = tab.id;
    this.#error = undefined;
    await tab.load();
  }

  /** Opens a scratch tab with no api, or one from history, and brings it to the front. */
  scratch(from?: Readonly<{ method: string; url: string }>): ScratchTab {
    const tab = new ScratchTab(this.#adhoc, this.#id());
    if (from) [tab.method, tab.url, tab.title] = [from.method, from.url, from.url];
    this.tabs.push(tab);
    this.activeId = tab.id;
    return tab;
  }

  activate(id: string): void {
    if (this.tabs.some((t) => t.id === id)) this.activeId = id;
  }

  /** Closes a tab; when it was in front, the tab that takes its place comes to the front. */
  close(id: string): void {
    const at = this.tabs.findIndex((t) => t.id === id);
    if (at < 0) return;
    const [gone] = this.tabs.splice(at, 1);
    if (gone?.kind === 'scratch') gone.stream.abort();
    if (this.activeId !== id) return;
    this.activeId = this.tabs[Math.min(at, this.tabs.length - 1)]?.id;
  }

  addVar(): void {
    this.call?.addVar();
  }

  removeVar(index: number): void {
    this.call?.removeVar(index);
  }

  async explain(): Promise<boolean> {
    return (await this.call?.explain()) ?? false;
  }

  async send(keep?: boolean): Promise<void> {
    const tab = this.tab;
    if (!tab) return;
    await tab.send(keep);
    this.sent++;
  }

  async saveAsTest(testId: string, fields: readonly string[]): Promise<string | undefined> {
    return this.call?.saveAsTest(testId, fields);
  }

  #id(): string {
    return `t${this.#next++}`;
  }
}

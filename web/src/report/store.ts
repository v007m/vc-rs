/** Where the data of a key comes from: `request` asks for it, and `failed` says it never will. */
export type Request = (key: string, failed: () => void) => void;

type Waiting = Readonly<{ promise: Promise<unknown>; resolve: (value: unknown) => void }>;

/**
 * The report's data by key (`summary`, `index`, `tests/<shard>`, `bodies/<hash>`). In the folder
 * layout each key is the script `data/<key>.js`, requested on first use, which calls `load`. In
 * the single-file layout every key is in one inflated bundle.
 */
export class DataStore {
  readonly #values = new Map<string, unknown>();
  readonly #waiting = new Map<string, Waiting>();
  readonly #request: Request;
  #bundle: Promise<Readonly<Record<string, unknown>>> | undefined;

  constructor(request: Request) {
    this.#request = request;
  }

  /** A data file's `VIB.load(key, value)` call. */
  load(key: string, value: unknown): void {
    this.#values.set(key, value);
    this.#waiting.get(key)?.resolve(value);
    this.#waiting.delete(key);
  }

  /** Serves every key from `bundle` from now on. */
  useBundle(bundle: Promise<Readonly<Record<string, unknown>>>): void {
    this.#bundle = bundle;
  }

  get(key: string): Promise<unknown> {
    if (this.#values.has(key)) return Promise.resolve(this.#values.get(key));
    if (this.#bundle) return this.#bundle.then((all) => fromBundle(all, key));
    return this.#waiting.get(key)?.promise ?? this.#ask(key);
  }

  #ask(key: string): Promise<unknown> {
    let resolve: (value: unknown) => void = () => {};
    let reject: (error: Error) => void = () => {};
    const promise = new Promise<unknown>((done, fail) => {
      resolve = done;
      reject = fail;
    });
    this.#waiting.set(key, { promise, resolve });
    this.#request(key, () => {
      this.#waiting.delete(key);
      reject(new Error(`The report's data/${key}.js could not be loaded.`));
    });
    return promise;
  }
}

function fromBundle(all: Readonly<Record<string, unknown>>, key: string): unknown {
  if (Object.hasOwn(all, key)) return all[key];
  throw new Error(`The report has no ${key}.`);
}

/** The single-file layout's data: base64 of gzipped JSON, one object of every key. */
export async function inflate(base64: string): Promise<Readonly<Record<string, unknown>>> {
  const bytes = Uint8Array.from(atob(base64.trim()), (c) => c.charCodeAt(0));
  const stream = new Response(bytes).body?.pipeThrough(new DecompressionStream('gzip'));
  return (await new Response(stream).json()) as Readonly<Record<string, unknown>>;
}

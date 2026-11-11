/**
 * The few WASI preview 1 calls the playground's `vc-playground.wasm` makes, for a Web Worker:
 * stdin from a buffer, stdout and stderr to a callback, the clocks, randomness, sleeping and
 * exit. Every other call answers ENOSYS, so a call the page cannot honour fails in Rust with an
 * error, never a crash. Pages has no cross-origin isolation, so there is no SharedArrayBuffer and
 * no `Atomics.wait`: a sleep waits in a loop, in the worker, off the page's thread.
 */

const SUCCESS = 0;
const EBADF = 8;
const EINVAL = 28;
const ENOSYS = 52;

const CLOCK_REALTIME = 0;
const CLOCK_MONOTONIC = 1;
const SUBSCRIPTION = 48;
const EVENT = 32;
const TAG_CLOCK = 0;
const ABSTIME = 1;
const RANDOM_CHUNK = 65536;

export type Stream = 'out' | 'err';

/** `proc_exit`, thrown to unwind out of `_start`. */
export class Exit extends Error {
  constructor(readonly code: number) {
    super(`exit ${code}`);
  }
}

export interface WasiOptions {
  /** What the program reads on stdin. */
  stdin: Uint8Array;
  /** Called with each piece of text the program writes. */
  write: (stream: Stream, text: string) => void;
}

/** Nanoseconds on the clock `id`: the wall clock (`Date.now()`), or the page's monotonic clock
 * (`performance.now()`). */
export function now(id: number): bigint {
  const ms = id === CLOCK_REALTIME ? Date.now() : performance.now();
  return BigInt(Math.round(ms * 1e6));
}

/** A clock subscription: which clock, its timeout in nanoseconds, and whether the timeout is an
 * absolute time on that clock rather than a span from now. */
export interface ClockWait {
  clock: number;
  timeout: bigint;
  absolute: boolean;
}

/** When `wait` is due on the monotonic clock: a span from now, an absolute monotonic time as it
 * is, or an absolute wall-clock time moved by how far it is from the wall clock now. */
export function due(
  wait: ClockWait,
  at = { realtime: now(CLOCK_REALTIME), monotonic: now(CLOCK_MONOTONIC) },
): bigint {
  if (!wait.absolute) return at.monotonic + wait.timeout;
  return wait.clock === CLOCK_REALTIME ? at.monotonic + (wait.timeout - at.realtime) : wait.timeout;
}

/** Waits until the monotonic clock reaches `deadline`; see the module note. */
function waitUntil(deadline: bigint): void {
  while (now(CLOCK_MONOTONIC) < deadline) {
    // Busy wait: a worker has no blocking sleep without SharedArrayBuffer.
  }
}

/** The WASI imports for one run, and the memory they read once the instance exists. */
export class Wasi {
  private memory: WebAssembly.Memory | undefined;
  private read = 0;
  private readonly decoders = { out: new TextDecoder(), err: new TextDecoder() };

  constructor(private readonly options: WasiOptions) {}

  /** The instance's memory, which the calls read and write. */
  bind(memory: WebAssembly.Memory): void {
    this.memory = memory;
  }

  /** Runs the program's `_start` and gives its exit code. */
  start(instance: WebAssembly.Instance): number {
    this.bind(instance.exports.memory as WebAssembly.Memory);
    try {
      (instance.exports._start as () => void)();
      return 0;
    } catch (error) {
      if (error instanceof Exit) return error.code;
      throw error;
    } finally {
      this.flush();
    }
  }

  /** The import object for `module`: the calls below, and ENOSYS for every other one. */
  imports(module: WebAssembly.Module): WebAssembly.Imports {
    const calls: Record<string, (...args: never[]) => number | void> = {
      args_sizes_get: (count: number, size: number) => this.zeros(count, size),
      args_get: () => SUCCESS,
      environ_sizes_get: (count: number, size: number) => this.zeros(count, size),
      environ_get: () => SUCCESS,
      clock_time_get: (id: number, _precision: bigint, out: number) => this.clock(id, out),
      random_get: (buffer: number, length: number) => this.random(buffer, length),
      fd_read: (fd: number, iovs: number, count: number, out: number) => this.fdRead(fd, iovs, count, out),
      fd_write: (fd: number, iovs: number, count: number, out: number) => this.fdWrite(fd, iovs, count, out),
      poll_oneoff: (subs: number, events: number, count: number, out: number) => this.poll(subs, events, count, out),
      sched_yield: () => SUCCESS,
      proc_exit: (code: number) => {
        throw new Exit(code);
      },
    };
    const wasi: Record<string, (...args: never[]) => number | void> = {};
    for (const entry of WebAssembly.Module.imports(module)) {
      if (entry.module === 'wasi_snapshot_preview1' && entry.kind === 'function') {
        wasi[entry.name] = calls[entry.name] ?? (() => ENOSYS);
      }
    }
    return { wasi_snapshot_preview1: wasi };
  }

  private view(): DataView {
    if (!this.memory) throw new Error('the program has no memory yet');
    return new DataView(this.memory.buffer);
  }

  private bytes(at: number, length: number): Uint8Array {
    return new Uint8Array(this.view().buffer, at, length);
  }

  private zeros(count: number, size: number): number {
    const view = this.view();
    view.setUint32(count, 0, true);
    view.setUint32(size, 0, true);
    return SUCCESS;
  }

  private clock(id: number, out: number): number {
    this.view().setBigUint64(out, now(id), true);
    return SUCCESS;
  }

  private random(buffer: number, length: number): number {
    for (let at = 0; at < length; at += RANDOM_CHUNK) {
      crypto.getRandomValues(this.bytes(buffer + at, Math.min(RANDOM_CHUNK, length - at)));
    }
    return SUCCESS;
  }

  private fdRead(fd: number, iovs: number, count: number, out: number): number {
    if (fd !== 0) return EBADF;
    const view = this.view();
    const stdin = this.options.stdin;
    let total = 0;
    for (let i = 0; i < count && this.read < stdin.length; i++) {
      const at = view.getUint32(iovs + i * 8, true);
      const length = view.getUint32(iovs + i * 8 + 4, true);
      const piece = stdin.subarray(this.read, this.read + length);
      this.bytes(at, piece.length).set(piece);
      this.read += piece.length;
      total += piece.length;
    }
    view.setUint32(out, total, true);
    return SUCCESS;
  }

  private fdWrite(fd: number, iovs: number, count: number, out: number): number {
    const stream: Stream | undefined = fd === 1 ? 'out' : fd === 2 ? 'err' : undefined;
    if (!stream) return EBADF;
    const view = this.view();
    let total = 0;
    for (let i = 0; i < count; i++) {
      const at = view.getUint32(iovs + i * 8, true);
      const length = view.getUint32(iovs + i * 8 + 4, true);
      const text = this.decoders[stream].decode(this.bytes(at, length), { stream: true });
      if (text) this.options.write(stream, text);
      total += length;
    }
    view.setUint32(out, total, true);
    return SUCCESS;
  }

  /** Sleeps until the earliest clock subscription is due, then reports each one that is; any
   * other subscription is ready at once. */
  private poll(subs: number, events: number, count: number, out: number): number {
    if (count === 0) return EINVAL;
    const view = this.view();
    const dues: bigint[] = [];
    for (let i = 0; i < count; i++) {
      const sub = subs + i * SUBSCRIPTION;
      if (view.getUint8(sub + 8) !== TAG_CLOCK) {
        dues.push(0n);
        continue;
      }
      const clock = view.getUint32(sub + 16, true);
      const timeout = view.getBigUint64(sub + 24, true);
      const absolute = (view.getUint16(sub + 40, true) & ABSTIME) !== 0;
      dues.push(due({ clock, timeout, absolute }));
    }
    waitUntil(dues.reduce((earliest, due) => (due < earliest ? due : earliest)));
    let fired = 0;
    for (let i = 0; i < count; i++) {
      if (dues[i] > now(CLOCK_MONOTONIC)) continue;
      const sub = subs + i * SUBSCRIPTION;
      const event = events + fired * EVENT;
      new Uint8Array(view.buffer, event, EVENT).fill(0);
      view.setBigUint64(event, view.getBigUint64(sub, true), true);
      view.setUint8(event + 10, view.getUint8(sub + 8));
      fired += 1;
    }
    view.setUint32(out, fired, true);
    return SUCCESS;
  }

  private flush(): void {
    for (const stream of ['out', 'err'] as const) {
      const text = this.decoders[stream].decode();
      if (text) this.options.write(stream, text);
    }
  }
}

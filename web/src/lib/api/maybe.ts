import { Code, ConnectError } from '@connectrpc/connect';

/**
 * What a seam over a newer RPC gives (D538): the value, or why there is none. An engine without
 * the RPC answers `unimplemented`; the seam turns that into `Unavailable` with the reason, and
 * the screen shows a quiet empty state, not an error.
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

/** `work`'s value, or `reason` when the engine does not have the RPC it calls. */
export async function unlessMissing<T>(reason: string, work: () => Promise<T>): Promise<Maybe<T>> {
  try {
    return available(await work());
  } catch (error) {
    if (ConnectError.from(error).code === Code.Unimplemented) return notYet(reason);
    throw error;
  }
}

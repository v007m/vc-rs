/**
 * The largest message the engine API takes: `vc ui` refuses a request body over 4 MiB (the
 * server's `envelope::MAX_MESSAGE`), so anything sent whole has to fit in it.
 */
export const maxMessageBytes = 4 * 1024 * 1024;

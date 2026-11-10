import { createContext } from 'svelte';
import type { RunSession } from './session.svelte.ts';

/** The app's run session: the layout sets it, and any screen that starts or shows a run gets it. */
export const [getRunSession, setRunSession] = createContext<RunSession>();

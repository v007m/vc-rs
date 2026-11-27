import { createContext } from 'svelte';
import type { VcsSession } from './session.svelte.ts';

/** The app's git pane state: the layout sets it; the Changes section and the status line read it. */
export const [getVcsSession, setVcsSession] = createContext<VcsSession>();

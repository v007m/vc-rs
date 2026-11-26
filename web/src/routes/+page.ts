import { redirect } from '@sveltejs/kit';

/** The app opens in Client (D512). */
export function load(): never {
  redirect(307, '/client');
}

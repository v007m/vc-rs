import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

/** The editor moved under Tests (D513); old links keep their `?file=`. */
export const load: PageLoad = ({ url }) => redirect(308, `/tests/editor${url.search}`);

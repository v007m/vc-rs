import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

/** Insights is Runs › Overview now (D513); old links keep their query. */
export const load: PageLoad = ({ url }) => redirect(308, `/runs/overview${url.search}`);

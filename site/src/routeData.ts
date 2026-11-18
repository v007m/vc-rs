import { defineRouteMiddleware } from '@astrojs/starlight/route-data';
import { plainTitle } from './lib/titles';

/** Drops the backticks of a key title from the `<title>` tag and `og:title`. */
export const onRequest = defineRouteMiddleware(({ locals }) => {
  for (const tag of locals.starlightRoute.head) {
    if (tag.tag === 'title' && tag.content) tag.content = plainTitle(tag.content);
    if (tag.tag === 'meta' && tag.attrs?.property === 'og:title' && typeof tag.attrs.content === 'string') {
      tag.attrs.content = plainTitle(tag.attrs.content);
    }
  }
});

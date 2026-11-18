/**
 * Page titles mark a key with backticks, as users type it: "`beforeAll` and `afterAll`". The H1,
 * sidebar and prev/next links show those spans in code font; everything else reads plain text.
 */

const CODE_SPAN = /`([^`]+)`/g;

/** Tags, and whole script and style elements, which pass through untouched. */
const MARKUP = /(<script\b[\s\S]*?<\/script>|<style\b[\s\S]*?<\/style>|<[^>]*>)/i;

/** The title without its backticks, for the `<title>` tag and link previews. */
export function plainTitle(title: string): string {
  return title.replace(CODE_SPAN, '$1');
}

/** Rendered HTML with every backticked span in its text wrapped in `<code>`. */
export function codeSpans(html: string): string {
  return html
    .split(MARKUP)
    .map((part, i) => (i % 2 === 1 ? part : part.replace(CODE_SPAN, '<code>$1</code>')))
    .join('');
}

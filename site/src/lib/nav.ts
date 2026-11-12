/**
 * The header's second row: the docs areas on every docs page, and the landing page's sections on
 * the landing page. One list each, so the header, the tests and the footer agree.
 */
const BASE = import.meta.env.BASE_URL.replace(/\/$/, '');

export interface NavLink {
  label: string;
  href: string;
}

/** A docs area: where its tab links, and the path prefixes whose pages belong to it. */
export interface DocsArea extends NavLink {
  match: string[];
}

const area = (label: string, page: string, ...match: string[]): DocsArea => ({
  label,
  href: `${BASE}/${page}`,
  match: match.map((m) => `${BASE}/${m}`),
});

/** In order; the first area whose prefix is longest wins, so `reference/cli/` beats `reference/`. */
export const DOCS_AREAS: DocsArea[] = [
  area('Get started', 'start/quickstart/', 'start/'),
  area('Walkthroughs', 'walkthroughs/rest-crud/', 'walkthroughs/'),
  area('Reference', 'reference/', 'reference/'),
  area('CLI', 'reference/cli/', 'reference/cli/'),
  area('Guides', 'guides/', 'guides/'),
  area('AI agents', 'agents/', 'agents/'),
  area('Playground', 'playground/', 'playground/'),
];

/** The area a page belongs to, by the longest matching prefix. */
export function areaFor(pathname: string): DocsArea | undefined {
  let best: DocsArea | undefined;
  let bestLength = 0;
  for (const a of DOCS_AREAS) {
    for (const prefix of a.match) {
      if (pathname.startsWith(prefix) && prefix.length > bestLength) {
        best = a;
        bestLength = prefix.length;
      }
    }
  }
  return best;
}

/** The landing page's sections, by the `id` of each section's heading. */
export const LANDING_SECTIONS: NavLink[] = [
  { label: 'A test', href: '#a-test' },
  { label: 'Features', href: '#features' },
  { label: 'How it works', href: '#how-it-works' },
  { label: 'AI agents', href: '#agents' },
  { label: 'Reports', href: '#reports' },
  { label: 'Get started', href: '#get-started' },
];

/** Where the landing page's "Docs" link goes. */
export const DOCS_HOME: NavLink = { label: 'Docs', href: `${BASE}/start/quickstart/` };

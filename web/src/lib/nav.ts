import type { RouteId } from '$app/types';

/** One screen in the side nav. Icons are the mockups' 16px stroke paths (`d` attributes). */
export interface NavItem {
  readonly route: RouteId;
  readonly label: string;
  readonly icon: readonly string[];
}

/** The screens built so far, in the mockups' nav order (brief-shell-v2). */
export const navItems: readonly NavItem[] = [
  {
    route: '/tests',
    label: 'Tests',
    icon: ['m2.5 4.5 1.5 1.5 2.5-3M2.5 11l1.5 1.5 2.5-3M9 5h4.5M9 11.5h4.5'],
  },
  {
    route: '/client',
    label: 'API client',
    icon: ['M2.5 5.5h9l-2.5-2.5M13.5 10.5h-9l2.5 2.5'],
  },
  { route: '/editor', label: 'Editor', icon: ['M10.5 2.5l3 3L6 13H3v-3zM9 4l3 3'] },
  {
    route: '/runs',
    label: 'Runs',
    icon: ['M8 2a6 6 0 1 0 0 12A6 6 0 0 0 8 2z', 'M6.5 5.5v5l4-2.5z'],
  },
  {
    route: '/agents',
    label: 'AI agents',
    icon: [
      'M5 5h6a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2z',
      'M8 2.5V5M6 8.5v1.5M10 8.5v1.5',
    ],
  },
];

/** Whether `pathname` is `route` or a page under it. */
export function isCurrent(pathname: string, route: RouteId): boolean {
  return pathname === route || pathname.startsWith(`${route}/`);
}

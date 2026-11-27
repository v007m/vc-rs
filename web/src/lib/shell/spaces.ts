import type { RouteId } from '$app/types';

/** A route with no params, so a sidebar row or a space's home links to it as it is. */
export type PlainRoute = Exclude<RouteId, `${string}[${string}`>;

/** The five spaces of the shell (D512), in rail order; ⌘1–⌘5 pick them in this order. */
export type SpaceKey = 'client' | 'tests' | 'runs' | 'agents' | 'settings';

/** One screen of a space, as its sidebar row and the top row's title. */
export interface View {
  readonly route: PlainRoute;
  /** The sidebar row. */
  readonly label: string;
  /** The top row's title and the page title. */
  readonly title: string;
}

/** A collapsible sidebar section (D515): a group of a space's views. */
export interface Section {
  /** Unique within the space; `<space>:<id>` is the key a closed section is saved under. */
  readonly id: string;
  readonly label: string;
  readonly views: readonly View[];
}

/** A space: its rail button, the screen it opens on and its sidebar. */
export interface Space {
  readonly key: SpaceKey;
  readonly label: string;
  /** An icon name of `Icon.svelte`. */
  readonly icon: IconName;
  readonly home: PlainRoute;
  /** Its shortcut with ⌘ (or Ctrl): `1`–`4`, and `,` for Settings. */
  readonly shortcut: string;
  /** Its Strata colour preset, the canvas and accent of `data-space` (D591). */
  readonly preset: SpacePreset;
  readonly sections: readonly Section[];
}

/** Strata's colour presets (strata/tokens.css). */
export type SpacePreset = 'lilac' | 'ember' | 'tide' | 'grove' | 'graphite';

/** The icons the shell draws (Icon.svelte holds their paths). */
export type IconName =
  | 'send'
  | 'flow'
  | 'runs'
  | 'bot'
  | 'gear'
  | 'columns'
  | 'rows'
  | 'search'
  | 'chevron'
  | 'x'
  | 'plus'
  | 'dots'
  | 'filter'
  | 'history'
  | 'import';

const view = (route: PlainRoute, label: string, title = label): View => ({ route, label, title });

/** The spaces in rail order; Settings is pinned at the bottom of the rail. */
export const spaces: readonly Space[] = [
  {
    key: 'client',
    preset: 'tide',
    label: 'Client',
    icon: 'send',
    home: '/client',
    shortcut: '1',
    sections: [
      { id: 'requests', label: 'Requests', views: [view('/client', 'Request', 'API client')] },
    ],
  },
  {
    key: 'tests',
    preset: 'lilac',
    label: 'Tests',
    icon: 'flow',
    home: '/tests',
    shortcut: '2',
    sections: [
      {
        id: 'views',
        label: 'Tests',
        views: [view('/tests', 'All tests', 'Tests'), view('/tests/editor', 'Editor')],
      },
    ],
  },
  {
    key: 'runs',
    preset: 'ember',
    label: 'Runs',
    icon: 'runs',
    home: '/runs',
    shortcut: '3',
    sections: [
      {
        id: 'views',
        label: 'Runs',
        views: [
          view('/runs/overview', 'Overview', 'Runs overview'),
          view('/runs', 'All runs', 'All runs'),
          view('/runs/schedules', 'Schedules'),
        ],
      },
    ],
  },
  {
    key: 'agents',
    preset: 'grove',
    label: 'Agents',
    icon: 'bot',
    home: '/agents',
    shortcut: '4',
    sections: [
      {
        id: 'views',
        label: 'Agents',
        views: [
          view('/agents', 'Activity', 'AI agents'),
          view('/agents/usage', 'Usage', 'Agent usage'),
        ],
      },
    ],
  },
  {
    key: 'settings',
    preset: 'graphite',
    label: 'Settings',
    icon: 'gear',
    home: '/settings',
    shortcut: ',',
    sections: [
      {
        id: 'project',
        label: 'Project',
        views: [
          view('/settings', 'Config', 'Settings'),
          view('/settings/alerts', 'Alerts'),
          view('/settings/plugins', 'Plugins'),
        ],
      },
      { id: 'user', label: 'You only', views: [view('/settings/user', 'Your settings')] },
      { id: 'app', label: 'This app', views: [view('/settings/appearance', 'Appearance')] },
    ],
  },
];

const [clientSpace] = spaces as [Space, ...Space[]];

const views = spaces.flatMap((space) =>
  space.sections.flatMap((section) => section.views.map((v) => ({ space, view: v }))),
);

/** Whether `pathname` is `route` or a page under it. */
export function isUnder(pathname: string, route: string): boolean {
  return pathname === route || pathname.startsWith(`${route}/`);
}

/** The view `pathname` shows: the one with the longest route it is under, or undefined. */
function match(pathname: string): { space: Space; view: View } | undefined {
  let best: { space: Space; view: View } | undefined;
  for (const entry of views) {
    if (!isUnder(pathname, entry.view.route)) continue;
    if (!best || entry.view.route.length > best.view.route.length) best = entry;
  }
  return best;
}

/** The space `pathname` belongs to; an unknown path counts as Client, the space the app opens in. */
export function spaceOf(pathname: string): Space {
  return match(pathname)?.space ?? clientSpace;
}

/** The view `pathname` shows, or undefined for a path no view has. */
export function viewOf(pathname: string): View | undefined {
  return match(pathname)?.view;
}

/**
 * The sidebar item `pathname` lights (D669): the view whose route it is, exactly. A page under
 * a view that has no item of its own (the live run, a load run, a stored run) lights none.
 */
export function navOf(pathname: string): View | undefined {
  const path = pathname.length > 1 ? pathname.replace(/\/$/, '') : pathname;
  return views.find((entry) => entry.view.route === path)?.view;
}

/** The space `key` names. */
export function spaceByKey(key: SpaceKey): Space {
  return spaces.find((space) => space.key === key) ?? clientSpace;
}

/** Whether `pathname` is a screen the shell knows, so it may be remembered as a last screen. */
export function isKnown(pathname: string): boolean {
  return match(pathname) !== undefined;
}

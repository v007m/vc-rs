/** Whose settings a Settings page shows: the project's, or yours on this machine alone. */
export type SettingsScope = 'project' | 'user';

const project = new Set(['/settings', '/settings/alerts', '/settings/plugins']);

/** The scope `pathname` is under, or undefined on a page of this app (Appearance). */
export function scopeOf(pathname: string): SettingsScope | undefined {
  const path = pathname.replace(/\/+$/, '') || '/';
  if (path === '/settings/user') return 'user';
  return project.has(path) ? 'project' : undefined;
}

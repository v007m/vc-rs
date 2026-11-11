/** Line icons for the landing page: the inner markup of a 24×24, stroke-drawn `<svg>`. */
export const icons = {
  check: '<circle cx="12" cy="12" r="9"/><path d="m8 12.5 2.5 2.5L16 9.5"/>',
  contract: '<path d="M7 3h7l4 4v14H7z"/><path d="M14 3v4h4"/><path d="m10 14.5 1.5 1.5 3.5-3.5"/>',
  load: '<path d="M4 18a8 8 0 1 1 16 0"/><path d="m12 18 4-6"/><path d="M8 10.5 7 9.5M12 8V6.5M16.5 9.5l-1 1"/>',
  terminal: '<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="m7 9.5 3 2.5-3 2.5"/><path d="M13 15h4"/>',
  browser: '<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M3 9h18"/><path d="M6.5 6.5h.01M9 6.5h.01"/><path d="m9 13.5 2 2 4-4"/>',
  agent: '<rect x="5" y="8" width="14" height="11" rx="3"/><path d="M12 4.5V8"/><circle cx="12" cy="3.5" r="1"/><path d="M9.5 13v1.5M14.5 13v1.5"/><path d="M2.5 12.5v2M21.5 12.5v2"/>',
  plug: '<path d="M9 3v5M15 3v5"/><path d="M6 8h12v3a6 6 0 0 1-12 0z"/><path d="M12 17v4"/>',
  git: '<circle cx="6" cy="6" r="2"/><circle cx="6" cy="18" r="2"/><circle cx="18" cy="8" r="2"/><path d="M6 8v8"/><path d="M18 10v1a4 4 0 0 1-4 4H8"/>',
  arrow: '<path d="M5 12h14"/><path d="m13 6 6 6-6 6"/>',
  copy: '<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V6a2 2 0 0 1 2-2h9"/>',
  done: '<path d="m5 12.5 4.5 4.5L19 7.5"/>',
  report: '<path d="M7 3h7l4 4v14H7z"/><path d="M14 3v4h4"/><path d="M10 12h5M10 15.5h5"/>',
} as const;

export type IconName = keyof typeof icons;

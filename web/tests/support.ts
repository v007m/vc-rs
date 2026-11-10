import { test } from '@playwright/test';

/** Skips the calling spec file until `vc ui` serves the web UI (C5); VC_E2E=1 turns it on. */
export function requiresVcUi(): void {
  test.skip(process.env.VC_E2E !== '1', 'needs `vc ui` (C5); set VC_E2E=1');
}

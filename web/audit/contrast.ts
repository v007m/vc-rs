import type { Page } from '@playwright/test';
import type { Ink } from './page.ts';

/**
 * Text and icon contrast against what is really behind them: the spec shoots the page with
 * all text and icons hidden, and this measures each ink, composited over the mean of the
 * pixels under it, against that mean. Glass over the gradient and the grain are in the shot,
 * so no token is trusted. It runs in a blank page, where a canvas can read the shot.
 */
export async function lowContrast(blank: Page, backdrop: Buffer, inks: Ink[]): Promise<string[]> {
  return blank.evaluate(
    async ({ png, inks }) => {
      const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
      const image = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
      const canvas = new OffscreenCanvas(image.width, image.height);
      const g = canvas.getContext('2d');
      if (!g) return ['contrast: no canvas'];
      g.drawImage(image, 0, 0);
      const lum = (c: number[]) => {
        const [r = 0, gr = 0, b = 0] = c.map((v) => {
          const s = v / 255;
          return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
        });
        return 0.2126 * r + 0.7152 * gr + 0.0722 * b;
      };
      const out = new Set<string>();
      for (const { box, ink, min, what } of inks) {
        const x = Math.max(0, Math.floor(box.x));
        const y = Math.max(0, Math.floor(box.y));
        const w = Math.min(image.width - x, Math.ceil(box.w));
        const h = Math.min(image.height - y, Math.ceil(box.h));
        if (w < 1 || h < 1) continue;
        const px = g.getImageData(x, y, w, h).data;
        const sum = [0, 0, 0];
        for (let i = 0; i < px.length; i += 4)
          for (let k = 0; k < 3; k++) sum[k] = (sum[k] ?? 0) + (px[i + k] ?? 0);
        const mean = sum.map((v) => v / (px.length / 4));
        const [r, gr, b, a] = ink;
        const text = [r, gr, b].map((v, k) => v * a + (mean[k] ?? 0) * (1 - a));
        const [one, two] = [lum(text), lum(mean)];
        const ratio = (Math.max(one, two) + 0.05) / (Math.min(one, two) + 0.05);
        if (ratio < min) out.add(`contrast: ${what} is ${ratio.toFixed(2)}:1, under ${min}:1`);
      }
      return [...out];
    },
    { png: backdrop.toString('base64'), inks },
  );
}

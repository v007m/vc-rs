import type { Page } from '@playwright/test';
import type { Edge, Ink } from './page.ts';

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

/**
 * An unchecked control's boundary against its backdrop (WCAG 1.4.11, D738), in the same
 * ink-free shot. At the middle of each side it reads the two pixels that straddle the edge and
 * the track 3px in, against the backdrop 3px out, and keeps the best of them; the control fails
 * when any side is under 3:1, since a boundary drawn on one side only does not outline it.
 */
export async function weakEdges(blank: Page, backdrop: Buffer, edges: Edge[]): Promise<string[]> {
  return blank.evaluate(
    async ({ png, edges }) => {
      const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
      const image = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
      const canvas = new OffscreenCanvas(image.width, image.height);
      const g = canvas.getContext('2d', { willReadFrequently: true });
      if (!g) return ['control-contrast: no canvas'];
      g.drawImage(image, 0, 0);
      // the mean luminance of three pixels along the edge (across it, they would blur it)
      const at = (x: number, y: number, along: boolean): number => {
        const [ax, ay] = [Math.round(x), Math.round(y)];
        const px = along
          ? g.getImageData(ax - 1, ay, 3, 1).data
          : g.getImageData(ax, ay - 1, 1, 3).data;
        const lin = (v: number) => {
          const s = v / 255;
          return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
        };
        let sum = 0;
        for (let i = 0; i < px.length; i += 4)
          sum +=
            0.2126 * lin(px[i] ?? 0) + 0.7152 * lin(px[i + 1] ?? 0) + 0.0722 * lin(px[i + 2] ?? 0);
        return sum / 3;
      };
      const ratio = (a: number, b: number) => (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
      const out = new Set<string>();
      for (const { box, what } of edges) {
        const [cx, cy] = [box.x + box.w / 2, box.y + box.h / 2];
        const [l, t] = [Math.floor(box.x), Math.floor(box.y)];
        const [r, b] = [Math.ceil(box.x + box.w) - 1, Math.ceil(box.y + box.h) - 1];
        // the middle of each side, on its edge pixel, with the step that points outward
        const sides: [number, number, number, number][] = [
          [cx, t, 0, -1],
          [cx, b, 0, 1],
          [l, cy, -1, 0],
          [r, cy, 1, 0],
        ];
        let worst = Infinity;
        for (const [x, y, dx, dy] of sides) {
          const along = dx === 0;
          const back = at(x + dx * 3, y + dy * 3, along);
          const inner = [0, 1, 3].map((k) => at(x - dx * k, y - dy * k, along));
          worst = Math.min(worst, Math.max(...inner.map((v) => ratio(v, back))));
        }
        if (worst < 3)
          out.add(`control-contrast: ${what} has an edge of ${worst.toFixed(2)}:1, under 3:1`);
      }
      return [...out];
    },
    { png: backdrop.toString('base64'), edges },
  );
}

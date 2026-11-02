// hooks/hostile.ts — safety probes. Each export is called separately by the host.
import type { CustomCheck } from "@vibranium/sdk";

interface Probe { name: string; ok: boolean; detail: string }

export const spin: CustomCheck = () => {
  let n: number = 0;
  while (true) { n++; }
};

export const hog: CustomCheck = () => {
  const keep: number[][] = [];
  for (;;) { keep.push(new Array<number>(1 << 20).fill(keep.length + 0.5)); }
};

export const escape: CustomCheck = () => {
  const g = globalThis as Record<string, unknown>;
  const probes: Probe[] = ["fetch", "XMLHttpRequest", "require", "process", "Deno", "std", "os", "WebSocket"]
    .map((name) => ({ name, ok: typeof g[name] !== "undefined", detail: typeof g[name] }));
  const reachable = probes.filter((p) => p.ok).map((p) => p.name);
  return reachable.length === 0 || `reachable: ${reachable.join(",")}`;
};

export const importFs = async (): Promise<string> => {
  const spec: string = "fs";
  const m: unknown = await import(spec);
  return `imported fs: ${typeof m}`;
};

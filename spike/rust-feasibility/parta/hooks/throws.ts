// hooks/throws.ts — error surfacing / source-map probe
import type { CustomCheck, HttpResponse } from "@vibranium/sdk";

type Item = { currency: string; amount: number };

function total(items: Item[]): number {
  return items.reduce((acc: number, i: Item) => acc + i.amount, 0);
}

export const totalIsPositive: CustomCheck = ({ response }: { response: HttpResponse }) => {
  const items: Item[] = response.body.lines; // bug: field is "items"
  return total(items) > 0;
};

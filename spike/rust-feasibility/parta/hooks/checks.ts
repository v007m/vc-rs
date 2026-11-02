// hooks/checks.ts
import type { CustomCheck } from "@vibranium/sdk";
export const currencyIsConsistent: CustomCheck = ({ response }) => {
  const c = new Set(response.body.items.map((i: { currency: string }) => i.currency));
  return c.size === 1 || `mixed currencies: ${[...c].join(", ")}`;
};

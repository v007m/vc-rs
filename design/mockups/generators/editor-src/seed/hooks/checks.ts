import type { CustomCheck } from "@vibranium/sdk";

// Every line of an order is charged in the same currency as the order.
export const currencyIsConsistent: CustomCheck = ({ response }) => {
  const currencies = new Set(response.body.items.map((item: { currency: string }) => item.currency));
  return currencies.size === 1 || `mixed currencies: ${[...currencies].join(", ")}`;
};

// Lists come back sorted by creation time, newest first.
export const newestFirst: CustomCheck = ({ response }) => {
  const times = response.body.items.map((item: { createdAt: string }) => Date.parse(item.createdAt));
  return times.every((t: number, i: number) => i === 0 || times[i - 1] >= t) || "items are not newest first";
};

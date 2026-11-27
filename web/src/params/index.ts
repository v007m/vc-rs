import { defineParams } from '@sveltejs/kit/params';

/** A run's number in a route (`/runs/12`, D644): a whole number from 1, without leading zeros. */
export function integer(param: string): number | undefined {
  return /^[1-9]\d{0,14}$/.test(param) ? Number(param) : undefined;
}

export const params = defineParams({ integer });

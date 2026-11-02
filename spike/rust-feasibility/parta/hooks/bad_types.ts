// hooks/bad_types.ts — should be rejected by tsc --noEmit in the user's repo
import type { BeforeRequestHook } from "@vibranium/sdk";
export const broken: BeforeRequestHook = ({ request }) => {
  return { ...request, headers: { ...request.headers, "x-n": 42 } };
};

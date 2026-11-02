// hooks/sign.ts
import type { BeforeRequestHook } from "@vibranium/sdk";
export const addSignature: BeforeRequestHook = ({ request, secret }) => {
  const sig = hmacSha256(secret("UPLOAD_KEY"), request.path); // provide a host function for this
  return { ...request, headers: { ...request.headers, "x-signature": sig } };
};

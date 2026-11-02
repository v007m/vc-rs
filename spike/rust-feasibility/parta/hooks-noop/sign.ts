import type { BeforeRequestHook } from "@vibranium/sdk";
export const addSignature: BeforeRequestHook = ({ request }) => request;

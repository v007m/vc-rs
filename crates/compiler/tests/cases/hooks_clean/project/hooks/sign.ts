import type { BeforeRequestHook } from "@vibranium/sdk";
import { hmacSha256Hex } from "vibranium:std";
import { header } from "./util";

export const sign: BeforeRequestHook = ({ request, secrets }) => {
  request.headers[header] = hmacSha256Hex(secrets.KEY, request.body ?? "");
};

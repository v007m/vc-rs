import { createHmac } from "node:crypto";
import { Request } from "@vibranium/sdk";
import axios from "axios";
import { header } from "./headers";

export async function sign({ request }: { request: Request }) {
  const { extra } = await import("./extra.ts");
  request.headers[header] = createHmac("sha256", "k").digest("hex") + extra + axios;
}

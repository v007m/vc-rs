import type { BeforeRequest } from "@vibranium/sdk";
import { createHmac } from "node:crypto";

// Signs upload requests the way the media gateway expects.
export const addUploadSignature: BeforeRequest = ({ request, secret }) => {
  const signature = createHmac("sha256", secret.UPLOAD_KEY).update(request.body ?? "").digest("hex");
  request.headers["x-upload-signature"] = signature;
};

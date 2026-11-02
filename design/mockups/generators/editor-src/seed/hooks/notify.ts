import type { RunHook } from "@vibranium/sdk";

// Posts a one-line summary of the run to the team channel.
export const postSummary: RunHook = async ({ run, secret }) => {
  const line = `${run.profile}: ${run.passed}/${run.total} passed, ${run.failed} failed in ${run.duration}`;
  await fetch(secret.SLACK_WEBHOOK, { method: "POST", body: JSON.stringify({ text: line }) });
};

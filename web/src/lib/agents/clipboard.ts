/** Copies `text` and says, for a status line, whether that worked. */
export async function copyText(text: string): Promise<string> {
  try {
    await navigator.clipboard.writeText(text);
    return 'Copied.';
  } catch {
    return 'Could not copy; select the text instead.';
  }
}

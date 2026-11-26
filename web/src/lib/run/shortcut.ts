/** Whether `event` is **Run…**'s chord: ⌘R on Apple, Ctrl+R elsewhere, with no other modifier (D635). */
export function isRunKey(
  event: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>,
): boolean {
  const mod = event.metaKey || event.ctrlKey;
  return mod && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'r';
}

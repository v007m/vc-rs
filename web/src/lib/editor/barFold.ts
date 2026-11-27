/**
 * How much of the editor bar folds into its … menu at a width (D669). A toolbar never wraps:
 * from the lowest priority up, Revert and Check fold first, then Diff, Fix all and Run file.
 * Save and the menu itself never fold. An unmeasured bar (width 0) shows everything.
 */
export type BarFold = 0 | 1 | 2;

/** Below this width Revert and Check fold into the menu. */
export const FOLD_SOME = 720;
/** Below this width Diff, Fix all and Run file fold too. */
export const FOLD_MORE = 540;

export function barFold(width: number): BarFold {
  if (width <= 0 || width >= FOLD_SOME) return 0;
  return width >= FOLD_MORE ? 1 : 2;
}

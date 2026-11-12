import { DefinitionDraft } from './draft.svelte.ts';
import { flowEdits, flowProblem, newStage, stagesOf, type Stage } from './flowForm.ts';
import type { WrittenDefinition } from './port.ts';

/**
 * The open flow test as the Client edits it (D251): its stages as its file writes them, each
 * with its loop, and saving the edits back through the guarded `EditFile`. A run runs the file
 * as saved, so the flow runs only once its edits are saved.
 */
export class FlowEditor extends DefinitionDraft {
  stages: Stage[] = $state([]);
  #base: Stage[] = $state([]);
  readonly edits = $derived(
    this.written ? flowEdits(this.written.pointer, this.#base, this.stages) : [],
  );
  readonly problem = $derived(flowProblem(this.stages));

  protected opened(written: WrittenDefinition): void {
    this.#base = stagesOf(written.node);
    this.stages = stagesOf(written.node);
  }

  discard(): void {
    this.stages = structuredClone($state.snapshot(this.#base));
  }

  /** Adds a stage calling `api` once, at the end. */
  addStage(api: string): void {
    this.stages.push(newStage(api));
  }

  removeStage(at: number): void {
    this.stages.splice(at, 1);
  }

  /** Moves the stage at `at` one place up (`-1`) or down (`1`); none past either end. */
  moveStage(at: number, by: -1 | 1): void {
    const to = at + by;
    if (to < 0 || to >= this.stages.length) return;
    const [stage] = this.stages.splice(at, 1);
    if (stage) this.stages.splice(to, 0, stage);
  }
}

// A linear undo/redo stack. Each entry is a pair of closures: `apply`
// performs the change, `revert` undoes it. Committing a new command
// while undone drops the redo tail, matching every editor's behavior.
// DOM-free and state-free, so Node tests it directly.
//
// This replaces the old pop-only history, which could only mean "remove
// the last shape" and so could never express redo or edits to an
// existing shape.
export class History {
  constructor() {
    this.stack = [];
    this.index = 0; // number of commands currently applied
  }

  // Run `apply` now and record it so it can be reverted and re-applied.
  commit(apply, revert) {
    this.stack.length = this.index; // a new action forks off the redo tail
    apply();
    this.stack.push({ apply, revert });
    this.index++;
  }

  undo() {
    if (this.index === 0) return false;
    this.index--;
    this.stack[this.index].revert();
    return true;
  }

  redo() {
    if (this.index >= this.stack.length) return false;
    this.stack[this.index].apply();
    this.index++;
    return true;
  }

  canUndo() {
    return this.index > 0;
  }

  canRedo() {
    return this.index < this.stack.length;
  }
}

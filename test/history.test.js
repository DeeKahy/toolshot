import { test } from "node:test";
import assert from "node:assert/strict";
import { History } from "../src/js/history.js";

// A tiny document the commands mutate, standing in for the shapes array.
function make() {
  const items = [];
  const h = new History();
  const add = (x) =>
    h.commit(
      () => items.push(x),
      () => items.splice(items.indexOf(x), 1)
    );
  return { items, h, add };
}

test("commit applies immediately", () => {
  const { items, add } = make();
  add("a");
  assert.deepEqual(items, ["a"]);
});

test("undo reverts, redo re-applies", () => {
  const { items, h, add } = make();
  add("a");
  add("b");
  assert.deepEqual(items, ["a", "b"]);

  assert.equal(h.undo(), true);
  assert.deepEqual(items, ["a"]);
  assert.equal(h.undo(), true);
  assert.deepEqual(items, []);

  assert.equal(h.redo(), true);
  assert.deepEqual(items, ["a"]);
  assert.equal(h.redo(), true);
  assert.deepEqual(items, ["a", "b"]);
});

test("undo past the start and redo past the end are no-ops", () => {
  const { h, add } = make();
  add("a");
  assert.equal(h.undo(), true);
  assert.equal(h.undo(), false);
  assert.equal(h.redo(), true);
  assert.equal(h.redo(), false);
});

test("a new commit after undo drops the redo tail", () => {
  const { items, h, add } = make();
  add("a");
  add("b");
  h.undo(); // items: ["a"], "b" is now redoable
  add("c"); // forks: "b" should be gone for good
  assert.deepEqual(items, ["a", "c"]);
  assert.equal(h.redo(), false); // nothing to redo
  assert.equal(h.canUndo(), true);
});

test("canUndo and canRedo track the stack position", () => {
  const { h, add } = make();
  assert.equal(h.canUndo(), false);
  assert.equal(h.canRedo(), false);
  add("a");
  assert.equal(h.canUndo(), true);
  assert.equal(h.canRedo(), false);
  h.undo();
  assert.equal(h.canUndo(), false);
  assert.equal(h.canRedo(), true);
});

import { test } from "node:test";
import assert from "node:assert/strict";
import { normalizeRect, arrowHead, pointInRect, distToSegment } from "../src/js/shapes.js";

test("normalizeRect handles a drag in any direction", () => {
  assert.deepEqual(normalizeRect({ x0: 30, y0: 40, x1: 10, y1: 10 }), {
    x: 10,
    y: 10,
    w: 20,
    h: 30,
  });
});

test("arrowHead points sit around the tip and the shaft stops short", () => {
  // Horizontal arrow, head length 10. Shaft should end before the tip.
  const a = arrowHead(0, 0, 100, 0, 10);
  assert.deepEqual(a.tip, { x: 100, y: 0 });
  assert.ok(a.shaftEnd.x < 100 && a.shaftEnd.x > 90);
  // The two wings straddle the arrow axis (one above, one below).
  assert.ok(Math.sign(a.left.y) === -Math.sign(a.right.y));
  assert.ok(a.left.y !== 0 && a.right.y !== 0);
  assert.ok(a.left.x < 100 && a.right.x < 100);
});

test("pointInRect respects padding", () => {
  const r = { x: 0, y: 0, w: 100, h: 100 };
  assert.equal(pointInRect(50, 50, r), true);
  assert.equal(pointInRect(-5, 50, r), false);
  assert.equal(pointInRect(-5, 50, r, 10), true);
});

test("distToSegment measures perpendicular and endpoint distances", () => {
  // Point above the middle of a horizontal segment.
  assert.equal(distToSegment(50, 10, 0, 0, 100, 0), 10);
  // Point past the end clamps to the endpoint.
  assert.equal(distToSegment(110, 0, 0, 0, 100, 0), 10);
  // Degenerate segment is just the distance to the point.
  assert.equal(distToSegment(3, 4, 0, 0, 0, 0), 5);
});

import { test } from "node:test";
import assert from "node:assert/strict";
import * as geo from "../src/js/geometry.js";

test("viewRect returns the full image when uncropped", () => {
  assert.deepEqual(geo.viewRect(null, 800, 600), { x: 0, y: 0, w: 800, h: 600 });
});

test("viewRect returns the crop when set", () => {
  const crop = { x: 10, y: 20, w: 100, h: 50 };
  assert.equal(geo.viewRect(crop, 800, 600), crop);
});

test("pretty padding is a tenth of the long edge, floored at 48", () => {
  assert.equal(geo.prettyPad(2000, 1000), 200);
  assert.equal(geo.prettyPad(100, 100), 48); // floor wins for small shots
});

test("pretty radius scales with width, floored at 8", () => {
  assert.equal(geo.prettyRadius(1800), 20);
  assert.equal(geo.prettyRadius(100), 8);
});

test("margin is zero unless pretty mode is on", () => {
  assert.equal(geo.margin(false, 2000, 1000), 0);
  assert.equal(geo.margin(true, 2000, 1000), 200);
});

test("strokeWidth scales with the capture and floors at 4", () => {
  assert.equal(geo.strokeWidth(3200), 10);
  assert.equal(geo.strokeWidth(320), 4);
  assert.equal(geo.strokeWidth(100), 4);
});

test("canvasToImage inverts the display scaling, margin and crop offset", () => {
  // Canvas bitmap 240 wide displayed at 120 (2x shrink), 20px margin, crop
  // origin at 5. A click 60px into the displayed canvas maps to:
  // 60 * (240/120) - 20 + 5 = 120 - 20 + 5 = 105.
  const rect = { left: 0, top: 0, width: 120, height: 120 };
  const p = geo.canvasToImage(60, 60, rect, 240, 240, 20, 5, 5);
  assert.deepEqual(p, { x: 105, y: 105 });
});

test("fitScale never magnifies past 1:1", () => {
  assert.equal(geo.fitScale(100, 100, 500, 500), 1);
  assert.equal(geo.fitScale(1000, 500, 500, 500), 0.5);
});

test("clampPan stops the content at the stage edge", () => {
  // content 1000, stage 400 -> max offset 300.
  assert.equal(geo.clampPan(500, 1000, 400), 300);
  assert.equal(geo.clampPan(-500, 1000, 400), -300);
  assert.equal(geo.clampPan(100, 1000, 400), 100);
  // content smaller than stage cannot pan at all.
  assert.equal(geo.clampPan(50, 200, 400), 0);
});

test("clampExportScale keeps dimensions under the 16k ceiling", () => {
  assert.equal(geo.clampExportScale(2, 500, 500), 2);
  // 4x of an 8000px side would be 32000, clamp to 2x (16000).
  assert.equal(geo.clampExportScale(4, 8000, 4000), 2);
  // Non-positive input falls back to 1.
  assert.equal(geo.clampExportScale(0, 500, 500), 1);
});

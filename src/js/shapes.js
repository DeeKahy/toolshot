// Pure shape geometry shared by the renderer and the tools. No DOM, so
// Node tests it. The canvas drawing itself lives in editor.js where it is
// coupled to the context and the frozen image; only the math is here, so
// a new tool never has to re-derive an arrow head or a normalized rect.

// A drag (two corners) to a top-left origin plus size.
export function normalizeRect(s) {
  return {
    x: Math.min(s.x0, s.x1),
    y: Math.min(s.y0, s.y1),
    w: Math.abs(s.x1 - s.x0),
    h: Math.abs(s.y1 - s.y0),
  };
}

// The three points of an arrow head plus where the shaft should stop so
// it does not poke through the head. `head` is the head length.
export function arrowHead(x0, y0, x1, y1, head) {
  const angle = Math.atan2(y1 - y0, x1 - x0);
  return {
    // Shaft stops short of the tip.
    shaftEnd: {
      x: x1 - head * 0.6 * Math.cos(angle),
      y: y1 - head * 0.6 * Math.sin(angle),
    },
    tip: { x: x1, y: y1 },
    left: {
      x: x1 - head * Math.cos(angle - 0.45),
      y: y1 - head * Math.sin(angle - 0.45),
    },
    right: {
      x: x1 - head * Math.cos(angle + 0.45),
      y: y1 - head * Math.sin(angle + 0.45),
    },
  };
}

// Is a point inside a normalized rect (with an optional padding)?
export function pointInRect(px, py, rect, pad = 0) {
  return (
    px >= rect.x - pad &&
    px <= rect.x + rect.w + pad &&
    py >= rect.y - pad &&
    py <= rect.y + rect.h + pad
  );
}

// Distance from a point to a line segment, for hit-testing strokes.
export function distToSegment(px, py, ax, ay, bx, by) {
  const dx = bx - ax;
  const dy = by - ay;
  const lenSq = dx * dx + dy * dy;
  let t = lenSq === 0 ? 0 : ((px - ax) * dx + (py - ay) * dy) / lenSq;
  t = Math.max(0, Math.min(1, t));
  const cx = ax + t * dx;
  const cy = ay + t * dy;
  return Math.hypot(px - cx, py - cy);
}

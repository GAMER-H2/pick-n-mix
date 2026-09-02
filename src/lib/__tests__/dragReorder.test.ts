import { describe, expect, it, vi } from "vitest";
import { ref } from "vue";
import { useDragReorder } from "../dragReorder";

/** A container with `count` rows stacked 20px apart, matching `[data-row]`. */
function makeContainer(count: number) {
  const container = document.createElement("div");
  for (let i = 0; i < count; i += 1) {
    const row = document.createElement("div");
    row.dataset.row = "";
    row.getBoundingClientRect = () =>
      ({ top: i * 20, bottom: i * 20 + 20, height: 20 }) as DOMRect;
    container.append(row);
  }
  return container;
}

function pointerEvent(clientY: number) {
  const target = document.createElement("button");
  target.setPointerCapture = vi.fn();
  target.releasePointerCapture = vi.fn();
  const event = new PointerEvent("pointerdown", { clientY });
  Object.defineProperty(event, "currentTarget", { value: target });
  return event;
}

describe("useDragReorder", () => {
  it("does nothing when dropped back on its own row", () => {
    const container = ref(makeContainer(5));
    const onMove = vi.fn();
    const drag = useDragReorder(container, onMove);

    drag.onHandleDown(pointerEvent(0), 2);
    drag.onHandleMove(pointerEvent(2 * 20 + 5)); // still inside row 2
    drag.onHandleUp(pointerEvent(2 * 20 + 5));

    expect(onMove).not.toHaveBeenCalled();
    expect(drag.dragFrom.value).toBeNull();
  });

  it("moving a row down accounts for the shift past the gap", () => {
    const container = ref(makeContainer(5));
    const onMove = vi.fn();
    const drag = useDragReorder(container, onMove);

    // Drag row 0 down to the gap after row 3 (y = 80, gap index 4).
    drag.onHandleDown(pointerEvent(0), 0);
    drag.onHandleMove(pointerEvent(85));
    drag.onHandleUp(pointerEvent(85));

    expect(onMove).toHaveBeenCalledWith(0, 3);
  });

  it("moving a row up drops it exactly at the target gap", () => {
    const container = ref(makeContainer(5));
    const onMove = vi.fn();
    const drag = useDragReorder(container, onMove);

    // Drag row 4 up to the gap before row 1 (y = 20, gap index 1).
    drag.onHandleDown(pointerEvent(4 * 20), 4);
    drag.onHandleMove(pointerEvent(20));
    drag.onHandleUp(pointerEvent(20));

    expect(onMove).toHaveBeenCalledWith(4, 1);
  });

  it("isDragging tracks the gesture and pointercancel aborts it without moving", () => {
    const container = ref(makeContainer(3));
    const onMove = vi.fn();
    const drag = useDragReorder(container, onMove);

    drag.onHandleDown(pointerEvent(0), 0);
    expect(drag.isDragging.value).toBe(true);

    drag.onHandleCancel();
    expect(drag.isDragging.value).toBe(false);
    expect(drag.dropAt.value).toBeNull();
    expect(onMove).not.toHaveBeenCalled();
  });
});

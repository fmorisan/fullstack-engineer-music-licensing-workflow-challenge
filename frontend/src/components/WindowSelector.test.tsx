// WindowSelector: drag handles move the playback window within the scene,
// clamped to [0, maxWindow] with a 1s minimum span; the song track shows
// the audible excerpt.

import { afterEach, afterAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { WindowSelector } from "./WindowSelector";

// jsdom rects are all zeros; the selector derives seconds from the track's
// bounding rect, so fake a 1000px track at x=0 → 16.67px per second (max 60s).
const rectSpy = vi
  .spyOn(HTMLElement.prototype, "getBoundingClientRect")
  .mockReturnValue({
    x: 0, y: 0, top: 0, left: 0, bottom: 34, right: 1000, width: 1000, height: 34,
    toJSON: () => ({}),
  } as DOMRect);

afterAll(() => {
  rectSpy.mockRestore();
});

afterEach(() => {
  vi.clearAllMocks();
  cleanup();
});

const drag = (handle: Element, fromX: number, toX: number) => {
  fireEvent.pointerDown(handle, { clientX: fromX });
  fireEvent.pointerMove(document, { clientX: toX });
  fireEvent.pointerUp(document, { clientX: toX });
};

describe("WindowSelector", () => {
  it("drags the start handle and keeps it before the end", () => {
    const onChange = vi.fn();
    render(
      <WindowSelector maxWindow={60} songLength={195} start={10} end={30} onChange={onChange} />,
    );
    drag(screen.getByLabelText("window start"), 100, 250); // 6s → 15s
    expect(onChange).toHaveBeenCalledWith(15, 30);
  });

  it("clamps the end handle to the scene window", () => {
    const onChange = vi.fn();
    render(
      <WindowSelector maxWindow={60} songLength={195} start={10} end={30} onChange={onChange} />,
    );
    drag(screen.getByLabelText("window end"), 300, 2000); // way past 60s
    expect(onChange).toHaveBeenLastCalledWith(10, 60);
  });

  it("enforces a 1s minimum span when handles collide", () => {
    const onChange = vi.fn();
    render(
      <WindowSelector maxWindow={60} songLength={195} start={10} end={30} onChange={onChange} />,
    );
    drag(screen.getByLabelText("window start"), 100, 800); // past the end
    expect(onChange).toHaveBeenLastCalledWith(29, 30);
  });

  it("shows how much of the song is audible", () => {
    render(
      <WindowSelector maxWindow={60} songLength={195} start={0} end={30} onChange={() => {}} />,
    );
    expect(screen.getByText(/0:30 of 3:15 audible/)).toBeInTheDocument();

    render(
      <WindowSelector maxWindow={90} songLength={60} start={0} end={90} onChange={() => {}} />,
    );
    expect(screen.getByText(/whole song/)).toBeInTheDocument();
  });
});

import { describe, expect, it } from "vitest";
import {
  MIN_BLOCK_SECS,
  addLane,
  blockEnd,
  cloneMix,
  deleteBlocks,
  locate,
  mixDuration,
  moveBlock,
  moveBlocks,
  removeLane,
  rulerStep,
  snapCandidates,
  snapDrag,
  snapTime,
  splitBlock,
  timecode,
  trimBlock,
} from "../masterMix";
import type { MasterMix, MixBlock } from "../types";

function block(id: string, startSecs: number, durationSecs: number): MixBlock {
  return {
    id,
    source: { kind: "entry", index: 0 },
    startSecs,
    offsetSecs: 0,
    durationSecs,
    gainDb: 0,
    fadeInSecs: 0,
    fadeOutSecs: 0,
    mixer: null,
    automation: [],
  };
}

function mix(): MasterMix {
  return {
    enabled: true,
    revision: 1,
    lanes: [
      { id: "l0", name: "One", muted: false, soloed: false, gainDb: 0, blocks: [block("a", 0, 100)] },
      { id: "l1", name: "Two", muted: false, soloed: false, gainDb: 0, blocks: [block("b", 100, 80)] },
    ],
  };
}

describe("moving blocks", () => {
  it("moves a block along its own lane", () => {
    const next = moveBlock(mix(), "a", 30);
    expect(locate(next, "a")!.block.startSecs).toBe(30);
    expect(locate(next, "a")!.laneIndex).toBe(0);
  });

  it("moves a block to another lane", () => {
    const next = moveBlock(mix(), "a", 10, 1);
    expect(locate(next, "a")!.laneIndex).toBe(1);
    expect(next.lanes[0].blocks).toHaveLength(0);
    expect(next.lanes[1].blocks.map((b) => b.id)).toEqual(["a", "b"]);
  });

  it("keeps a lane's blocks in time order after a move", () => {
    const next = moveBlock(mix(), "a", 500, 1);
    expect(next.lanes[1].blocks.map((b) => b.id)).toEqual(["b", "a"]);
  });

  it("never lets a block start before zero", () => {
    expect(locate(moveBlock(mix(), "a", -50), "a")!.block.startSecs).toBe(0);
  });

  it("lets blocks overlap, because that is what a crossfade is", () => {
    const next = moveBlock(mix(), "b", 90);
    expect(locate(next, "b")!.block.startSecs).toBe(90);
    expect(blockEnd(locate(next, "a")!.block)).toBe(100);
  });

  it("ignores a move of a block that is not there", () => {
    const before = mix();
    expect(moveBlock(before, "nope", 5)).toBe(before);
  });

  it("does nothing when asked for a lane that does not exist", () => {
    const before = mix();
    expect(moveBlock(before, "a", 5, 9)).toBe(before);
  });
});

describe("moving a selection", () => {
  it("shifts everything selected by the same amount", () => {
    const next = moveBlocks(mix(), ["a", "b"], 25);
    expect(locate(next, "a")!.block.startSecs).toBe(25);
    expect(locate(next, "b")!.block.startSecs).toBe(125);
  });

  it("stops the whole group at zero rather than collapsing it", () => {
    // 'a' is already at 0, so the group cannot move left at all — 'b' must
    // keep its 100 s spacing rather than sliding onto 'a'.
    const next = moveBlocks(mix(), ["a", "b"], -40);
    expect(locate(next, "a")!.block.startSecs).toBe(0);
    expect(locate(next, "b")!.block.startSecs).toBe(100);
  });

  it("clamps a lane change to the lanes that exist", () => {
    const next = moveBlocks(mix(), ["a", "b"], 0, 5);
    expect(locate(next, "a")!.laneIndex).toBe(0);
    expect(locate(next, "b")!.laneIndex).toBe(1);
  });

  it("moves a selection down one lane when there is room", () => {
    const next = moveBlocks(mix(), ["a"], 0, 1);
    expect(locate(next, "a")!.laneIndex).toBe(1);
  });
});

describe("trimming", () => {
  it("dragging the right edge shortens the block", () => {
    const next = trimBlock(mix(), "a", "end", 60);
    expect(locate(next, "a")!.block.durationSecs).toBe(60);
  });

  it("dragging the right edge cannot run past the end of the song", () => {
    const next = trimBlock(mix(), "a", "end", 500, 120);
    expect(locate(next, "a")!.block.durationSecs).toBe(120);
  });

  it("dragging the left edge keeps the audio under the cursor still", () => {
    const next = trimBlock(mix(), "b", "start", 130);
    const trimmed = locate(next, "b")!.block;
    expect(trimmed.startSecs).toBe(130);
    // Thirty seconds later in the timeline means thirty seconds later in the
    // song, or the waveform would appear to slide sideways.
    expect(trimmed.offsetSecs).toBe(30);
    expect(trimmed.durationSecs).toBe(50);
    expect(blockEnd(trimmed)).toBe(180);
  });

  it("dragging the left edge cannot expose audio before the song started", () => {
    const next = trimBlock(mix(), "b", "start", 10);
    const trimmed = locate(next, "b")!.block;
    // 'b' begins at the song's own start, so it cannot be pulled any earlier.
    expect(trimmed.startSecs).toBe(100);
    expect(trimmed.offsetSecs).toBe(0);
  });

  it("never trims a block out of existence", () => {
    const next = trimBlock(mix(), "a", "end", -10);
    expect(locate(next, "a")!.block.durationSecs).toBe(MIN_BLOCK_SECS);
  });

  it("shrinks fades that no longer fit", () => {
    const before = mix();
    before.lanes[0].blocks[0].fadeInSecs = 20;
    before.lanes[0].blocks[0].fadeOutSecs = 20;
    const next = trimBlock(before, "a", "end", 10);
    const trimmed = locate(next, "a")!.block;
    expect(trimmed.fadeInSecs).toBe(10);
    expect(trimmed.fadeOutSecs).toBe(0);
  });
});

describe("the blade", () => {
  it("cuts a block into two that together cover the original", () => {
    const next = splitBlock(mix(), "a", 40);
    const blocks = next.lanes[0].blocks;
    expect(blocks).toHaveLength(2);
    expect(blocks[0].startSecs).toBe(0);
    expect(blocks[0].durationSecs).toBe(40);
    expect(blocks[1].startSecs).toBe(40);
    expect(blocks[1].durationSecs).toBe(60);
    expect(blocks[1].offsetSecs).toBe(40);
    expect(blocks[0].id).not.toBe(blocks[1].id);
  });

  it("carries the offset through when cutting an already-cut block", () => {
    let next = splitBlock(mix(), "a", 40);
    const rightId = next.lanes[0].blocks[1].id;
    next = splitBlock(next, rightId, 70);
    const blocks = next.lanes[0].blocks;
    expect(blocks).toHaveLength(3);
    expect(blocks[2].offsetSecs).toBe(70);
    expect(blocks[2].durationSecs).toBe(30);
  });

  it("refuses a cut outside the block, or one that would leave a sliver", () => {
    const before = mix();
    expect(splitBlock(before, "a", 0)).toBe(before);
    expect(splitBlock(before, "a", 100)).toBe(before);
    expect(splitBlock(before, "a", 500)).toBe(before);
    expect(splitBlock(before, "a", MIN_BLOCK_SECS / 2)).toBe(before);
  });

  it("splits the automation envelope with the block", () => {
    const before = mix();
    before.lanes[0].blocks[0].automation = [
      { atSecs: 10, gainDb: 0, curve: 1 },
      { atSecs: 60, gainDb: -6, curve: 1 },
    ];
    const next = splitBlock(before, "a", 40);
    expect(next.lanes[0].blocks[0].automation.map((p) => p.atSecs)).toEqual([10]);
    // Points on the right-hand half are rebased, because automation is
    // measured from the block's own start.
    expect(next.lanes[0].blocks[1].automation.map((p) => p.atSecs)).toEqual([20]);
  });
});

describe("lanes", () => {
  it("adds an empty lane at the bottom", () => {
    const next = addLane(mix(), "Custom");
    expect(next.lanes).toHaveLength(3);
    expect(next.lanes[2].name).toBe("Custom");
    expect(next.lanes[2].blocks).toEqual([]);
  });

  it("removes a lane and everything on it", () => {
    const next = removeLane(mix(), 0);
    expect(next.lanes).toHaveLength(1);
    expect(locate(next, "a")).toBeNull();
  });

  it("ignores a removal that is out of range", () => {
    const before = mix();
    expect(removeLane(before, 7)).toBe(before);
  });
});

describe("snapping", () => {
  it("offers every other block's edges but not the dragged one's", () => {
    const candidates = snapCandidates(mix(), new Set(["a"]));
    expect(candidates).toContain(100);
    expect(candidates).toContain(180);
    expect(candidates).toContain(0);
    expect(candidates).toHaveLength(3);
  });

  it("takes the nearest candidate inside the tolerance", () => {
    expect(snapTime(98, [0, 100, 180], 5)).toBe(100);
    expect(snapTime(90, [0, 100, 180], 5)).toBe(90);
  });

  it("snaps a dragged block by whichever of its edges is closer", () => {
    // Dropping a 20 s block so its *end* lands on 100 is the common way to
    // butt one song against the next.
    expect(snapDrag(78, 20, [100], 5)).toBe(80);
    expect(snapDrag(102, 20, [100], 5)).toBe(100);
    expect(snapDrag(50, 20, [100], 5)).toBe(50);
  });
});

describe("housekeeping", () => {
  it("deletes several blocks at once", () => {
    const next = deleteBlocks(mix(), ["a", "b"]);
    expect(mixDuration(next)).toBe(0);
  });

  it("reports the mix's length as its last ending block", () => {
    expect(mixDuration(mix())).toBe(180);
  });

  it("clones deeply enough that editing a copy cannot touch the original", () => {
    const before = mix();
    const copy = cloneMix(before);
    copy.lanes[0].blocks[0].startSecs = 999;
    copy.lanes[0].name = "changed";
    expect(before.lanes[0].blocks[0].startSecs).toBe(0);
    expect(before.lanes[0].name).toBe("One");
  });

  it("formats timecode with milliseconds and hours only when needed", () => {
    expect(timecode(0)).toBe("0:00.000");
    expect(timecode(64.25)).toBe("1:04.250");
    expect(timecode(3661.5)).toBe("1:01:01.500");
    expect(timecode(-1)).toBe("0:00.000");
  });

  it("picks ruler steps that stay round as the zoom changes", () => {
    expect(rulerStep(1)).toBe(120);
    expect(rulerStep(10)).toBe(10);
    expect(rulerStep(200)).toBe(0.5);
  });
});

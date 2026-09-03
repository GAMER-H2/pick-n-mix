import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useBounceStore } from "../bounce";
import type { BounceOptions } from "@/lib/types";

const bounceMasterMix = vi.fn();

vi.mock("@/lib/api", () => ({
  bounceMasterMix: (...args: unknown[]) => bounceMasterMix(...args),
}));

const options: BounceOptions = {
  format: "wav",
  sampleRate: 48000,
  wavBitDepth: 24,
  flacCompression: 5,
  mp3Bitrate: 320,
};

describe("background bounces", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    bounceMasterMix.mockReset().mockResolvedValue("bounce_1");
  });

  it("tracks a render from start to finish", async () => {
    const bounce = useBounceStore();
    await bounce.start("pl_1", "Evening", "/tmp/evening.wav", options);

    expect(bounce.active).toBe(true);
    bounce.onProgress("bounce_1", 0.5);
    expect(bounce.jobs[0].fraction).toBe(0.5);

    bounce.onFinished("bounce_1", "/tmp/evening.wav", null);
    expect(bounce.jobs[0].done).toBe(true);
    expect(bounce.jobs[0].fraction).toBe(1);
    expect(bounce.active).toBe(false);
  });

  it("keeps a failure's message and does not claim it finished at 100%", async () => {
    const bounce = useBounceStore();
    await bounce.start("pl_1", "Evening", "/tmp/evening.wav", options);
    bounce.onProgress("bounce_1", 0.25);
    bounce.onFinished("bounce_1", "/tmp/evening.wav", "the disk is full");

    expect(bounce.jobs[0].error).toBe("the disk is full");
    expect(bounce.jobs[0].fraction).toBe(0.25);
  });

  /** Events arrive off a worker thread, so a late one must not reopen a job. */
  it("ignores progress that arrives after the job has stopped", async () => {
    const bounce = useBounceStore();
    await bounce.start("pl_1", "Evening", "/tmp/evening.wav", options);
    bounce.onFinished("bounce_1", "/tmp/evening.wav", null);
    bounce.onProgress("bounce_1", 0.3);

    expect(bounce.jobs[0].fraction).toBe(1);
    expect(bounce.jobs[0].done).toBe(true);
  });

  it("runs two renders at once and clears only what has stopped", async () => {
    const bounce = useBounceStore();
    await bounce.start("pl_1", "Evening", "/tmp/a.wav", options);
    bounceMasterMix.mockResolvedValue("bounce_2");
    await bounce.start("pl_2", "Morning", "/tmp/b.wav", options);
    expect(bounce.running).toHaveLength(2);

    bounce.onFinished("bounce_1", "/tmp/a.wav", null);
    bounce.dismissFinished();
    expect(bounce.jobs).toHaveLength(1);
    expect(bounce.jobs[0].name).toBe("Morning");
  });
});

import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import QueueList from "../QueueList.vue";
import type { Track } from "@/lib/types";

function track(id: string, title: string): Track {
  return {
    id,
    sourceId: "local",
    location: `/m/${id}.flac`,
    title,
    artist: "Artist",
    albumArtist: "Artist",
    album: "Album",
    trackNumber: 1,
    discNumber: 1,
    year: 2020,
    genre: null,
    durationSecs: 200,
    sampleRate: 44100,
    channels: 2,
    bitsPerSample: 16,
    bitrateKbps: 900,
    fileSize: 1,
    format: "FLAC",
    artworkId: null,
    musicbrainzRecordingId: null,
    musicbrainzReleaseId: null,
    gainDb: null,
    addedAt: 0,
    fileCount: 1,
    missingFileCount: 0,
    effectiveFileId: `${id}-file`,
    preferredFileId: null,
  };
}

function mountList(
  props: Partial<InstanceType<typeof QueueList>["$props"]> = {},
  slots: Parameters<typeof mount>[1] extends infer Options
    ? Options extends { slots?: infer Slots }
      ? Slots
      : never
    : never = {},
) {
  return mount(QueueList, {
    props: {
      items: [{ kind: "track", track: track("t1", "One") }],
      currentIndex: null,
      ...props,
    },
    slots,
    global: {
      stubs: {
        Artwork: true,
        PnmIcon: true,
      },
    },
  });
}

describe("QueueList", () => {
  it("keeps queue controls and default row content by default", async () => {
    const wrapper = mountList();

    expect(wrapper.find(".row__grip").exists()).toBe(true);
    expect(wrapper.find(".row__subtitle").text()).toBe("Artist · Album");
    expect(wrapper.find(".row__duration").text()).toBe("3:20");

    const remove = wrapper.get(".row__remove");
    expect(remove.attributes("aria-label")).toBe("Remove from queue");
    await remove.trigger("click");
    expect(wrapper.emitted("remove")).toEqual([[0]]);
  });

  it("hides reorder and remove affordances in reusable read-only modes", async () => {
    const wrapper = mountList({ reorderable: false, removable: false });

    expect(wrapper.find(".row__grip").exists()).toBe(false);
    expect(wrapper.find(".queue-list__drop").exists()).toBe(false);
    expect(wrapper.find(".row__remove").exists()).toBe(false);

    await wrapper.get(".row").trigger("dblclick");
    expect(wrapper.emitted("play")).toEqual([[0]]);
    expect(wrapper.emitted("move")).toBeUndefined();
  });

  it("renders removed rows as disabled while still allowing removal", async () => {
    const wrapper = mountList({ items: [{ kind: "track", track: null }], removeLabel: "Remove from history" });
    const row = wrapper.get(".row");

    expect(row.get(".row__title").text()).toBe("Removed track");
    expect(row.get(".row__subtitle").text()).toBe("No longer in library");
    expect(row.get(".row__art").attributes()).toHaveProperty("disabled");
    expect(row.find(".row__duration").exists()).toBe(false);

    await row.trigger("dblclick");
    await row.trigger("contextmenu");
    expect(wrapper.emitted("play")).toBeUndefined();
    expect(wrapper.emitted("menu")).toBeUndefined();

    const remove = row.get(".row__remove");
    expect(remove.attributes("aria-label")).toBe("Remove from history");
    await remove.trigger("click");
    expect(wrapper.emitted("remove")).toEqual([[0]]);
  });

  it("exposes track and index to subtitle and meta slots", () => {
    const wrapper = mountList(
      {},
      {
        subtitle: ({ track: item, index }: { track: Track | null; index: number }) =>
          `${index}:${item?.artist}`,
        meta: ({ track: item, index }: { track: Track | null; index: number }) =>
          `${item?.format}:${index}`,
      },
    );

    expect(wrapper.get(".row__subtitle").text()).toBe("0:Artist");
    expect(wrapper.get(".row").text()).toContain("FLAC:0");
    expect(wrapper.find(".row__duration").exists()).toBe(false);
  });
});

/**
 * A mix in the queue is one block. Its songs are listed but are not rows: no
 * grip, no drop target between them, and clicking one jumps to that point in
 * the arrangement rather than to an entry of its own.
 */
describe("QueueList with a master mix in it", () => {
  const mix = {
    playlistId: "pl_1",
    name: "Evening",
    artwork: null,
    artworkIds: [],
    durationSecs: 600,
    chapters: [
      { startSecs: 0, title: "One", artist: "A" },
      { startSecs: 300, title: "Two", artist: "B" },
    ],
  };

  function mountWithMix(currentIndex: number | null, positionSecs = 0) {
    return mount(QueueList, {
      props: {
        items: [
          { kind: "mix", mix } as const,
          { kind: "track", track: track("t1", "After") } as const,
        ],
        currentIndex,
        positionSecs,
      },
      global: { stubs: { Artwork: true, PlaylistArtwork: true, PnmIcon: true } },
    });
  }

  it("draws the mix as one block with its songs inside", () => {
    const wrapper = mountWithMix(0);
    expect(wrapper.findAll(".mix-block")).toHaveLength(1);
    expect(wrapper.findAll(".mix-block__chapter")).toHaveLength(2);
    // One grip for the whole mix, one for the song queued after it: none on
    // the chapters, which cannot be reordered.
    expect(wrapper.findAll(".row__grip")).toHaveLength(2);
    expect(wrapper.text()).toContain("Master mix · 2 songs");
  });

  it("marks the song the mix has reached", () => {
    const wrapper = mountWithMix(0, 320);
    const chapters = wrapper.findAll(".mix-block__chapter");
    expect(chapters[0].classes()).not.toContain("is-current");
    expect(chapters[1].classes()).toContain("is-current");
  });

  it("marks nothing when the mix is not the entry playing", () => {
    const wrapper = mountWithMix(1, 320);
    const marked = wrapper.findAll(".mix-block__chapter.is-current");
    expect(marked).toHaveLength(0);
  });

  it("asks to play the mix from a song's own position", async () => {
    const wrapper = mountWithMix(0);
    await wrapper.findAll(".mix-block__chapter")[1].trigger("click");
    expect(wrapper.emitted("play")).toEqual([[0, 300]]);
  });
});

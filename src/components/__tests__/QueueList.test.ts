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
      items: [track("t1", "One")],
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
    const wrapper = mountList({ items: [null], removeLabel: "Remove from history" });
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

import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import CollectionHeader from "../collections/CollectionHeader.vue";

function action(wrapper: ReturnType<typeof mount>, label: string) {
  return wrapper.findAll("button").find((button) => button.text() === label);
}

describe("CollectionHeader shuffle guard", () => {
  it("disables only Shuffle when shuffleDisabled is set", () => {
    const wrapper = mount(CollectionHeader, {
      props: { title: "Playlist", shuffleDisabled: true },
      global: { stubs: { PlaylistArtwork: true, PnmIcon: true } },
    });

    expect(action(wrapper, "Shuffle")?.attributes("disabled")).toBeDefined();
    expect(action(wrapper, "Play")?.attributes("disabled")).toBeUndefined();
    expect(wrapper.get("[aria-label='Mixer settings for this collection']").attributes("disabled"))
      .toBeUndefined();
    expect(wrapper.get("[aria-label='More actions']").attributes("disabled")).toBeUndefined();
  });

  it("keeps Shuffle enabled when the optional prop is omitted", () => {
    const wrapper = mount(CollectionHeader, {
      props: { title: "Album" },
      global: { stubs: { PlaylistArtwork: true, PnmIcon: true } },
    });

    expect(action(wrapper, "Shuffle")?.attributes("disabled")).toBeUndefined();
  });
});

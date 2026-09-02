import { createRouter, createWebHashHistory } from "vue-router";
import { trackNavigation } from "./lib/navigation";
import { trackScrollPositions } from "./lib/viewState";

/**
 * Hash history: the app is served from a custom scheme inside the webview,
 * where path-based history has no server to fall back on.
 */
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", name: "home", component: () => import("./views/HomeView.vue") },
    { path: "/library", name: "library", component: () => import("./views/LibraryView.vue") },
    {
      path: "/now-playing",
      name: "nowPlaying",
      component: () => import("./views/NowPlayingView.vue"),
    },
    {
      path: "/playlist/:id",
      name: "playlist",
      component: () => import("./views/PlaylistView.vue"),
    },
    { path: "/mix/:kind", name: "mix", component: () => import("./views/MixView.vue") },
    { path: "/album/:id", name: "album", component: () => import("./views/AlbumView.vue") },
    { path: "/artist/:id", name: "artist", component: () => import("./views/ArtistView.vue") },
    { path: "/:pathMatch(.*)*", redirect: "/" },
  ],
  scrollBehavior: () => ({ top: 0 }),
});

trackNavigation(router);
trackScrollPositions(router);

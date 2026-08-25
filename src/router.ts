import { createRouter, createWebHashHistory } from "vue-router";
import { trackNavigation } from "./lib/navigation";

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
      path: "/playlist/:id",
      name: "playlist",
      component: () => import("./views/PlaylistView.vue"),
    },
    { path: "/album/:id", name: "album", component: () => import("./views/AlbumView.vue") },
    { path: "/artist/:id", name: "artist", component: () => import("./views/ArtistView.vue") },
    { path: "/:pathMatch(.*)*", redirect: "/" },
  ],
  scrollBehavior: () => ({ top: 0 }),
});

trackNavigation(router);

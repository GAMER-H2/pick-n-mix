import { ref, watch, type Ref } from "vue";
import { useRoute } from "vue-router";

/**
 * Watches a route param and re-runs an async loader each time it changes,
 * including once immediately. The `loading` flag is raised for the whole
 * attempt and always lowered again, so callers can show an empty state only
 * once loading has genuinely finished.
 */
export function useRouteParamLoader(
  param: string,
  load: (value: string) => Promise<void>,
): { loading: Ref<boolean> } {
  const route = useRoute();
  const loading = ref(false);

  watch(
    () => route.params[param],
    async (value) => {
      if (typeof value !== "string") return;
      loading.value = true;
      try {
        await load(value);
      } finally {
        loading.value = false;
      }
    },
    { immediate: true },
  );

  return { loading };
}

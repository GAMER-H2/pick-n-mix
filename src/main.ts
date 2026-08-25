import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import { router } from "./router";
import "./styles/theme.css";

/**
 * Surface failures instead of leaving a blank window.
 *
 * A desktop app has no address bar and no obvious way to open a console, so an
 * unhandled error would otherwise show as nothing at all.
 */
function showFatal(message: string, detail?: string) {
  const root = document.getElementById("app");
  if (!root) return;
  root.innerHTML = "";

  const panel = document.createElement("div");
  panel.setAttribute("role", "alert");
  panel.style.cssText =
    "position:fixed;inset:0;display:flex;flex-direction:column;gap:12px;" +
    "align-items:center;justify-content:center;padding:40px;text-align:center;" +
    "font:13px/1.5 -apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;" +
    "background:#161618;color:#f5f5f7;";

  const title = document.createElement("h1");
  title.textContent = "Pick n Mix hit a problem";
  title.style.cssText = "margin:0;font-size:19px;font-weight:600;";

  const body = document.createElement("p");
  body.textContent = message;
  body.style.cssText = "margin:0;max-width:560px;color:#ff8a3d;";

  panel.append(title, body);

  if (detail) {
    const pre = document.createElement("pre");
    pre.textContent = detail;
    pre.style.cssText =
      "max-width:720px;max-height:40vh;overflow:auto;padding:12px;border-radius:8px;" +
      "background:#0e0e10;color:#a1a1a8;font-size:11px;text-align:left;user-select:text;";
    panel.append(pre);
  }

  root.append(panel);
}

window.addEventListener("error", (event) => {
  showFatal(event.message, event.error?.stack);
});

window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason;
  showFatal(
    reason instanceof Error ? reason.message : String(reason),
    reason instanceof Error ? reason.stack : undefined,
  );
});

const app = createApp(App);

app.config.errorHandler = (error, _instance, info) => {
  console.error(error);
  showFatal(
    error instanceof Error ? error.message : String(error),
    `${info}\n\n${error instanceof Error ? (error.stack ?? "") : ""}`,
  );
};

app.use(createPinia()).use(router).mount("#app");

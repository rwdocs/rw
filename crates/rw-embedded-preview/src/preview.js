import { mountRw } from "/lib/embed.js";

const root = document.getElementById("rw-root");
const themeBtn = document.getElementById("theme-toggle");
const darkMq = window.matchMedia("(prefers-color-scheme: dark)");

// Theme cycling: auto -> light -> dark -> auto
const themes = ["auto", "light", "dark"];
let themeIndex = 0;

function applyShellTheme() {
  const colorScheme = themes[themeIndex];
  document.body.classList.toggle(
    "dark-shell",
    colorScheme === "dark" ||
      (colorScheme === "auto" && darkMq.matches),
  );
}

// Mount the viewer once on page load
applyShellTheme();

// Opt-in host-catalog simulation (used by e2e tests, off by default). When a
// test sets `window.__RW_CATALOG_RESOLVER__`, map each documentation section ref
// (`kind:namespace/name`) to the URL where that entity's docs live — the way a
// Backstage host resolves catalog entities. A real host can only resolve
// entities registered in its catalog, so refs listed in
// `window.__RW_UNMAPPED_REFS__` are left unresolved on purpose, forcing the
// viewer to fall back to the nearest mapped ancestor in the ref's anchor chain.
// Without the opt-in flag, `rw serve --embedded` passes no
// resolver, so cross-section links resolve locally with no host base URL
// prepended.
function resolveSectionRefs(refs) {
  const unmapped = new Set(window.__RW_UNMAPPED_REFS__ || []);
  const map = {};
  for (const ref of refs) {
    if (unmapped.has(ref)) continue;
    const [kind, rest] = ref.split(":");
    const [namespace, name] = (rest || "").split("/");
    map[ref] = `/catalog/${namespace}/${kind}/${name}/docs`;
  }
  return Promise.resolve(map);
}

const currentInstance = mountRw(root, {
  apiBaseUrl: "/_api",
  embedded: true,
  colorScheme: themes[themeIndex],
  initialPath: (window.location.pathname || "/") + window.location.hash,
  resolveSectionRefs: window.__RW_CATALOG_RESOLVER__ ? resolveSectionRefs : undefined,
  onNavigate: (path) => {
    window.history.pushState({}, "", path);
  },
});

themeBtn.textContent = "Theme: " + themes[themeIndex];

// Update shell when OS preference changes (auto mode)
darkMq.addEventListener("change", () => applyShellTheme());

themeBtn.addEventListener("click", () => {
  themeIndex = (themeIndex + 1) % themes.length;
  applyShellTheme();
  currentInstance.setColorScheme(themes[themeIndex]);
  themeBtn.textContent = "Theme: " + themes[themeIndex];
});

// Handle browser back/forward
window.addEventListener("popstate", () => {
  currentInstance.navigateTo((window.location.pathname || "/") + window.location.hash);
});

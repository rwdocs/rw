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
    colorScheme === "dark" || (colorScheme === "auto" && darkMq.matches),
  );
}

// Mount the viewer once on page load
applyShellTheme();

// The default root ref. A site that names its root otherwise still loads: the
// server scopes an unknown ref to the root.
const sectionRef = "section:default/root";

function catalogUrl(ref) {
  const [kind, rest] = ref.split(":");
  const [namespace, name] = (rest || "").split("/");
  return `/catalog/${namespace}/${kind}/${name}/docs`;
}

// Opt-in host-catalog simulation (used by e2e tests, off by default). When a
// test sets `window.__RW_CATALOG_RESOLVER__`, map each documentation section ref
// to the URL where that entity's docs live — the way a Backstage host resolves
// catalog entities — and serve the root's docs under its own catalog URL. A
// real host can only resolve entities registered in its catalog, so refs listed
// in `window.__RW_UNMAPPED_REFS__` are left unresolved on purpose, forcing the
// viewer to fall back to the nearest mapped ancestor in the ref's anchor chain.
// Without the opt-in flag, `rw serve --embedded` passes no resolver, so the
// root's docs live at `/` and cross-section links resolve locally.
const catalog = Boolean(window.__RW_CATALOG_RESOLVER__);
const base = catalog ? catalogUrl(sectionRef) : "";

function resolveSectionRefs(refs) {
  const unmapped = new Set(window.__RW_UNMAPPED_REFS__ || []);
  const map = {};
  for (const ref of refs) {
    if (!unmapped.has(ref)) map[ref] = catalogUrl(ref);
  }
  return Promise.resolve(map);
}

function viewerPath() {
  return (window.location.pathname.slice(base.length) || "/") + window.location.hash;
}

const currentInstance = mountRw(root, {
  apiBaseUrl: "/_api",
  embedded: true,
  colorScheme: themes[themeIndex],
  initialPath: viewerPath(),
  sectionRef,
  resolveSectionRefs: catalog ? resolveSectionRefs : undefined,
  onNavigate: (path) => {
    // `navigateTo` on Back/Forward lands here too; pushing would drop Forward.
    const { pathname, search, hash } = window.location;
    if (path !== pathname + search + hash) window.history.pushState({}, "", path);
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
  currentInstance.navigateTo(viewerPath());
});

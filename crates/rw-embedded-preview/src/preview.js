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

const currentInstance = mountRw(root, {
  apiBaseUrl: "/api",
  embedded: true,
  colorScheme: themes[themeIndex],
  initialPath: window.location.pathname || "/",
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
  currentInstance.navigateTo(window.location.pathname || "/");
});

import { mountRw } from "/lib/embed.js";

const root = document.getElementById("rw-root");
const themeBtn = document.getElementById("theme-toggle");
const darkMq = window.matchMedia("(prefers-color-scheme: dark)");

let initialPath = window.location.pathname || "/";

// Theme cycling: auto -> light -> dark -> auto
const themes = ["auto", "light", "dark"];
let themeIndex = 0;
let currentInstance = null;

function applyShellTheme() {
  const colorScheme = themes[themeIndex];
  document.body.classList.toggle("dark-shell",
    colorScheme === "dark" ||
    (colorScheme === "auto" && darkMq.matches)
  );
}

function mountViewer() {
  if (currentInstance) currentInstance.destroy();

  const colorScheme = themes[themeIndex];

  applyShellTheme();

  currentInstance = mountRw(root, {
    apiBaseUrl: "/api",
    embedded: true,
    colorScheme: colorScheme,
    initialPath: initialPath,
    onNavigate: (path) => {
      initialPath = path;
      window.history.pushState({}, "", path);
    },
  });

  themeBtn.textContent = "Theme: " + colorScheme;
}

// Update shell when OS preference changes (auto mode)
darkMq.addEventListener("change", () => applyShellTheme());

themeBtn.addEventListener("click", () => {
  themeIndex = (themeIndex + 1) % themes.length;
  mountViewer();
});

// Handle browser back/forward
window.addEventListener("popstate", () => {
  initialPath = window.location.pathname || "/";
  mountViewer();
});

mountViewer();

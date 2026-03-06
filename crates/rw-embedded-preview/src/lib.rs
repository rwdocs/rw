//! Embedded preview shell for RW documentation engine.
//!
//! Serves a self-contained HTML page that wraps the RW viewer in a
//! minimal host-app shell for visual testing of embedded mode.
//! Replaces the normal SPA frontend with a Backstage-like shell
//! that embeds the viewer via `mountRw()`.

use axum::http::{StatusCode, header};
use axum::response::Response;

/// Serve the embedded preview HTML page.
///
/// Returns the same page regardless of the path — the JS extracts
/// the document path from the URL and passes it as `initialPath`.
pub async fn preview_page() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(PREVIEW_HTML.into())
        .unwrap()
}

/// Serve the preview page JavaScript as an external script.
///
/// Separated from the HTML to comply with Content-Security-Policy
/// `script-src 'self'` (inline scripts are blocked).
pub async fn preview_script() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/javascript; charset=utf-8")
        .body(PREVIEW_JS.into())
        .unwrap()
}

const PREVIEW_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Embedded Preview</title>
<link rel="stylesheet" href="/lib/embed.css">
<style>
  *, *::before, *::after { box-sizing: border-box; }

  body {
    margin: 0;
    height: 100vh;
    display: flex;
    flex-direction: column;
  }

  .shell-header, .shell-sidebar, .shell-header *, .shell-sidebar * {
    margin: 0;
    padding: 0;
  }

  /* Shell elements use system font; the RW viewer inherits its own font from embed.css */
  .shell-header, .shell-sidebar {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  }

  /* Header */
  .shell-header {
    height: 64px;
    background: #333;
    display: flex;
    align-items: center;
    padding: 0 24px;
    flex-shrink: 0;
    gap: 16px;
  }

  .shell-header-logo {
    color: #fff;
    font-size: 20px;
    font-weight: 700;
    letter-spacing: -0.5px;
  }

  .shell-header-spacer { flex: 1; }

  .shell-theme-toggle {
    background: rgba(255,255,255,0.15);
    border: none;
    color: #fff;
    padding: 6px 14px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
    font-family: inherit;
  }
  .shell-theme-toggle:hover { background: rgba(255,255,255,0.25); }

  /* Body layout */
  .shell-body {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  /* Sidebar */
  .shell-sidebar {
    width: 250px;
    background: #fff;
    border-right: 1px solid #E0E0E0;
    padding: 16px 0;
    flex-shrink: 0;
    overflow-y: auto;
  }
  .dark-shell .shell-sidebar {
    background: #272727;
    border-right-color: #444;
  }

  .shell-sidebar-item {
    padding: 10px 24px;
    font-size: 14px;
    color: #666;
    cursor: default;
  }
  .dark-shell .shell-sidebar-item { color: #999; }

  .shell-sidebar-item.active {
    color: #1F5493;
    font-weight: 600;
    background: #E8F0FE;
  }
  .dark-shell .shell-sidebar-item.active {
    color: #90CAF9;
    background: rgba(144,202,249,0.1);
  }

  .shell-sidebar-section {
    padding: 8px 24px 4px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: #999;
    font-weight: 600;
  }
  .dark-shell .shell-sidebar-section { color: #666; }

  /* Content area */
  .shell-content {
    flex: 1;
    overflow: auto;
    position: relative;
  }
</style>
</head>
<body>
  <header class="shell-header">
    <div class="shell-header-logo">Host App</div>
    <div class="shell-header-spacer"></div>
    <button class="shell-theme-toggle" id="theme-toggle">Theme: auto</button>
  </header>

  <div class="shell-body">
    <nav class="shell-sidebar">
      <div class="shell-sidebar-section">Navigation</div>
      <div class="shell-sidebar-item">Home</div>
      <div class="shell-sidebar-item">Dashboard</div>
      <div class="shell-sidebar-item active">Docs</div>
      <div class="shell-sidebar-item">Search</div>
      <div class="shell-sidebar-section">Admin</div>
      <div class="shell-sidebar-item">Settings</div>
    </nav>

    <div class="shell-content" id="rw-root"></div>
  </div>

  <script type="module" src="/__embedded_preview.js"></script>
</body>
</html>
"#;

const PREVIEW_JS: &str = r#"import { mountRw } from "/lib/embed.js";

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
"#;

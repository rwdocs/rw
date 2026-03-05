//! Embedded preview page handler.
//!
//! Serves a self-contained HTML page that wraps the RW viewer in a
//! minimal Backstage-like shell for visual testing of embedded mode.

use axum::http::{StatusCode, header};
use axum::response::Response;

/// Serve the embedded preview HTML page.
///
/// Returns the same page regardless of the path — the JS extracts
/// the document path from the URL and passes it as `initialPath`.
pub(crate) async fn preview_page() -> Response {
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
pub(crate) async fn preview_script() -> Response {
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
<title>RW Embedded Preview</title>
<link rel="stylesheet" href="/lib/embed.css">
<style>
  *, *::before, *::after { box-sizing: border-box; }

  body {
    margin: 0;
    height: 100vh;
    display: flex;
    flex-direction: column;
  }

  .bs-header, .bs-sidebar, .bs-header *, .bs-sidebar * {
    margin: 0;
    padding: 0;
  }

  /* Shell elements use system font; the RW viewer inherits its own font from embed.css */
  .bs-header, .bs-sidebar {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  }

  /* Header */
  .bs-header {
    height: 64px;
    background: #333;
    display: flex;
    align-items: center;
    padding: 0 24px;
    flex-shrink: 0;
    gap: 16px;
  }

  .bs-header-logo {
    color: #fff;
    font-size: 20px;
    font-weight: 700;
    letter-spacing: -0.5px;
  }

  .bs-header-spacer { flex: 1; }

  .bs-theme-toggle {
    background: rgba(255,255,255,0.15);
    border: none;
    color: #fff;
    padding: 6px 14px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
    font-family: inherit;
  }
  .bs-theme-toggle:hover { background: rgba(255,255,255,0.25); }

  /* Body layout */
  .bs-body {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  /* Sidebar */
  .bs-sidebar {
    width: 250px;
    background: #fff;
    border-right: 1px solid #E0E0E0;
    padding: 16px 0;
    flex-shrink: 0;
    overflow-y: auto;
  }
  .dark-shell .bs-sidebar {
    background: #272727;
    border-right-color: #444;
  }

  .bs-sidebar-item {
    padding: 10px 24px;
    font-size: 14px;
    color: #666;
    cursor: default;
  }
  .dark-shell .bs-sidebar-item { color: #999; }

  .bs-sidebar-item.active {
    color: #1F5493;
    font-weight: 600;
    background: #E8F0FE;
  }
  .dark-shell .bs-sidebar-item.active {
    color: #90CAF9;
    background: rgba(144,202,249,0.1);
  }

  .bs-sidebar-section {
    padding: 8px 24px 4px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: #999;
    font-weight: 600;
  }
  .dark-shell .bs-sidebar-section { color: #666; }

  /* Content area */
  .bs-content {
    flex: 1;
    overflow: hidden;
    position: relative;
  }
</style>
</head>
<body>
  <header class="bs-header">
    <div class="bs-header-logo">Backstage</div>
    <div class="bs-header-spacer"></div>
    <button class="bs-theme-toggle" id="theme-toggle">Theme: auto</button>
  </header>

  <div class="bs-body">
    <nav class="bs-sidebar">
      <div class="bs-sidebar-section">Menu</div>
      <div class="bs-sidebar-item">Home</div>
      <div class="bs-sidebar-item">APIs</div>
      <div class="bs-sidebar-item active">Docs</div>
      <div class="bs-sidebar-item">Tech Radar</div>
      <div class="bs-sidebar-section">Admin</div>
      <div class="bs-sidebar-item">Settings</div>
    </nav>

    <div class="bs-content" id="rw-root"></div>
  </div>

  <script type="module" src="/_preview/preview.js"></script>
</body>
</html>
"#;

const PREVIEW_JS: &str = r#"import { mountRw } from "/lib/embed.js";

const PREVIEW_PREFIX = "/_preview";
const root = document.getElementById("rw-root");
const themeBtn = document.getElementById("theme-toggle");

// Extract initial path from URL
const fullPath = window.location.pathname;
let initialPath = fullPath.startsWith(PREVIEW_PREFIX)
  ? fullPath.slice(PREVIEW_PREFIX.length) || "/"
  : "/";

// Theme cycling: auto -> light -> dark -> auto
const themes = ["auto", "light", "dark"];
let themeIndex = 0;
let currentInstance = null;

function mountViewer() {
  if (currentInstance) currentInstance.destroy();

  const colorScheme = themes[themeIndex];

  // Apply shell theme
  document.body.classList.toggle("dark-shell",
    colorScheme === "dark" ||
    (colorScheme === "auto" && window.matchMedia("(prefers-color-scheme: dark)").matches)
  );

  currentInstance = mountRw(root, {
    apiBaseUrl: "/api",
    embedded: true,
    colorScheme: colorScheme,
    initialPath: initialPath,
    onNavigate: (path) => {
      initialPath = path;
      window.history.pushState({}, "", PREVIEW_PREFIX + path);
    },
  });

  themeBtn.textContent = "Theme: " + colorScheme;
}

themeBtn.addEventListener("click", () => {
  themeIndex = (themeIndex + 1) % themes.length;
  mountViewer();
});

// Handle browser back/forward
window.addEventListener("popstate", () => {
  const p = window.location.pathname;
  initialPath = p.startsWith(PREVIEW_PREFIX)
    ? p.slice(PREVIEW_PREFIX.length) || "/"
    : "/";
  mountViewer();
});

mountViewer();
"#;

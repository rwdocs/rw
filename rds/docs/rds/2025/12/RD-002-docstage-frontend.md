# RD-002: Docstage Frontend

## Overview

Svelte-based standalone SPA frontend for Docstage documentation engine. Consumes the
backend API (RD-001) to render documentation pages with navigation, table of contents,
and breadcrumbs.

**Tagline:** "Where documentation takes the stage"

## Problem Statement

The Docstage backend (Phase 4 complete) provides JSON API for page rendering and
navigation, but lacks a user-facing frontend. Users need a web interface to browse
and read documentation.

## Goals

1. Standalone SPA served by Docstage backend.
2. Responsive navigation sidebar with collapsible tree structure.
3. Page rendering with table of contents and breadcrumbs.
4. Clean, minimal design inspired by [Stripe's documentation](https://docs.stripe.com/).
5. Prepared for live reload integration (Phase 5).

## Non-Goals (This RD)

- Backstage plugin integration (separate project).
- Client-side syntax highlighting (backend provides pre-highlighted HTML).
- Dark mode / theme switching.
- Client-side search.
- PDF export.
- Edit-on-GitHub links.

## Architecture

### High-Level Structure

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Browser                                                                    │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  Svelte SPA                                                          │   │
│  │                                                                      │   │
│  │  ┌──────────────┐  ┌──────────────────────────────────────────────┐ │   │
│  │  │  Navigation  │  │  Content Area                                 │ │   │
│  │  │  Sidebar     │  │  ┌────────────────────────┐  ┌─────────────┐ │ │   │
│  │  │              │  │  │  Breadcrumbs           │  │  ToC        │ │ │   │
│  │  │  - Tree      │  │  ├────────────────────────┤  │  Sidebar    │ │ │   │
│  │  │  - Collapse  │  │  │  Page Content          │  │             │ │ │   │
│  │  │  - Active    │  │  │  (HTML from API)       │  │  - Scroll   │ │ │   │
│  │  │    state     │  │  │                        │  │    spy      │ │ │   │
│  │  │              │  │  │                        │  │             │ │ │   │
│  │  └──────────────┘  │  └────────────────────────┘  └─────────────┘ │ │   │
│  │                    └──────────────────────────────────────────────┘ │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  Docstage Backend                                                           │
│                                                                             │
│  GET /api/navigation      → Navigation tree                                 │
│  GET /api/pages/{path}    → Page content + meta + toc + breadcrumbs        │
│  GET /                    → SPA index.html (static file serving)           │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Component Hierarchy

```
App.svelte
├── Layout.svelte
│   ├── NavigationSidebar.svelte
│   │   ├── NavTree.svelte (recursive)
│   │   └── NavItem.svelte
│   ├── ContentArea.svelte
│   │   ├── Breadcrumbs.svelte
│   │   └── PageContent.svelte
│   └── TocSidebar.svelte
│       └── TocItem.svelte
└── stores/
    ├── navigation.ts      (navigation tree state)
    └── page.ts            (current page state)
```

### Directory Structure

```
frontend/
├── package.json
├── vite.config.ts
├── tailwind.config.js
├── postcss.config.js
├── tsconfig.json
├── index.html               (entry point)
├── src/
│   ├── main.ts              (app bootstrap)
│   ├── App.svelte           (root component with router)
│   ├── app.css              (Tailwind imports)
│   ├── routes.ts            (route definitions)
│   ├── components/
│   │   ├── Layout.svelte
│   │   ├── NavigationSidebar.svelte
│   │   ├── NavTree.svelte
│   │   ├── NavItem.svelte
│   │   ├── ContentArea.svelte
│   │   ├── Breadcrumbs.svelte
│   │   ├── PageContent.svelte
│   │   ├── TocSidebar.svelte
│   │   └── TocItem.svelte
│   ├── pages/
│   │   ├── Home.svelte      (index page)
│   │   ├── Page.svelte      (documentation page)
│   │   └── NotFound.svelte  (404 page)
│   ├── stores/
│   │   ├── navigation.ts
│   │   └── page.ts
│   ├── api/
│   │   └── client.ts        (API client)
│   └── types/
│       └── index.ts         (TypeScript interfaces)
├── public/
│   └── favicon.png
└── dist/                    (build output)
```

## API Integration

### TypeScript Interfaces

```typescript
// Navigation types (from GET /api/navigation)
interface NavItem {
    title: string;
    path: string;
    children?: NavItem[];
}

interface NavigationTree {
    items: NavItem[];
}

// Page types (from GET /api/pages/{path})
interface PageMeta {
    title: string;
    path: string;
    source_file: string;
    last_modified: string;  // ISO 8601
}

interface Breadcrumb {
    title: string;
    path: string;
}

interface TocEntry {
    level: number;  // 2-6 (h2-h6)
    title: string;
    id: string;
}

interface PageResponse {
    meta: PageMeta;
    breadcrumbs: Breadcrumb[];
    toc: TocEntry[];
    content: string;  // HTML
}
```

### API Client

```typescript
// src/api/client.ts
const API_BASE = '/api';

export async function fetchNavigation(): Promise<NavigationTree> {
    const response = await fetch(`${API_BASE}/navigation`);
    if (!response.ok) throw new Error('Failed to fetch navigation');
    return response.json();
}

export async function fetchPage(path: string): Promise<PageResponse> {
    const response = await fetch(`${API_BASE}/pages/${path}`);
    if (!response.ok) {
        if (response.status === 404) throw new NotFoundError(path);
        throw new Error('Failed to fetch page');
    }
    return response.json();
}
```

## Component Specifications

### NavigationSidebar

- Displays collapsible tree structure.
- Highlights active page and expands parent sections.
- Persists collapse state in localStorage.
- Mobile: toggleable drawer with hamburger menu.

**Behavior:**

1. On mount: fetch navigation tree, restore collapse state from localStorage.
2. On navigation: expand path to active page, scroll active item into view.
3. On click: navigate to page, collapse/expand children if present.

### TocSidebar

- Displays table of contents for current page.
- Scroll spy: highlights current section based on scroll position.
- Sticky positioning on desktop, hidden on mobile.

**Behavior:**

1. Render ToC entries as indented list (level 2 = no indent, level 3 = 1 level, etc.).
2. On scroll: use IntersectionObserver to detect visible headings, highlight topmost.
3. On click: smooth scroll to heading anchor.

### PageContent

- Renders HTML content from API.
- Applies typography styles via Tailwind prose classes.
- Handles internal links for SPA navigation.

**Behavior:**

1. Insert HTML content using `{@html content}`.
2. Intercept clicks on internal links (`<a href="/...">`), use svelte-spa-router's `push()`.
3. Apply `prose` classes for consistent typography.

### Breadcrumbs

- Horizontal list of navigation path segments.
- Each segment is a link except the last (current page).

## Styling

### Design Inspiration

The UI design is inspired by [Stripe's documentation](https://docs.stripe.com/):

- Clean, spacious layout with generous whitespace
- Left sidebar navigation with subtle hover states
- Right-aligned table of contents
- Clear typographic hierarchy
- Minimal color palette (mostly grayscale with accent colors for links/actions)
- Smooth transitions and micro-interactions

### Tailwind Configuration

```javascript
// tailwind.config.js
module.exports = {
    content: ['./src/**/*.{html,js,svelte,ts}'],
    theme: {
        extend: {
            colors: {
                // Custom colors if needed
            },
        },
    },
    plugins: [
        require('@tailwindcss/typography'),
    ],
};
```

### Layout Dimensions

- Navigation sidebar: 280px fixed width on desktop.
- ToC sidebar: 240px fixed width on desktop.
- Content area: remaining width, max-width 800px centered.
- Mobile breakpoint: 768px (md).

### Typography

Use Tailwind Typography plugin (`@tailwindcss/typography`) for prose content:

```html
<article class="prose prose-slate max-w-none">
    {@html content}
</article>
```

## Routing

### Native Router (No Library)

Uses the browser's History API with a simple Svelte store - no external routing library
needed. Based on [The Last SPA Router You'll Need](https://snugug.com/musings/the-last-spa-router-you-ll-need/).

```typescript
// src/stores/router.ts
import { writable } from "svelte/store";

export const path = writable(window.location.pathname);

export function goto(newPath: string) {
    window.history.pushState({}, "", newPath);
    path.set(newPath);
}

export function initRouter() {
    // Handle browser back/forward
    window.addEventListener("popstate", () => {
        path.set(window.location.pathname);
    });

    // Intercept link clicks for SPA navigation
    document.addEventListener("click", (e) => {
        const anchor = (e.target as HTMLElement).closest("a");
        if (!anchor) return;

        const href = anchor.getAttribute("href");
        if (!href || href.startsWith("http") || href.startsWith("#")) return;

        e.preventDefault();
        goto(href);
    });
}
```

### App Component with Router

```svelte
<!-- src/App.svelte -->
<script lang="ts">
    import { onMount } from "svelte";
    import { path, initRouter } from "./stores/router";
    import Layout from "./components/Layout.svelte";
    import Home from "./pages/Home.svelte";
    import Page from "./pages/Page.svelte";
    import NotFound from "./pages/NotFound.svelte";

    onMount(() => initRouter());

    const getRoute = (p: string) => {
        if (p === "/") return "home";
        if (p.startsWith("/docs")) return "page";
        return "notfound";
    };

    let route = $derived(getRoute($path));
</script>

<Layout>
    {#if route === "home"}
        <Home />
    {:else if route === "page"}
        <Page />
    {:else}
        <NotFound />
    {/if}
</Layout>
```

### Page Component

```svelte
<!-- src/pages/Page.svelte -->
<script lang="ts">
    import { path } from "../stores/router";
    import { page } from "../stores/page";

    let docPath = $derived($path.replace(/^\/docs\/?/, ""));

    $effect(() => {
        page.load(docPath);
    });
</script>

{#if $page.loading}
    <p>Loading...</p>
{:else if $page.error}
    <p>Error: {$page.error}</p>
{:else if $page.data}
    <article class="prose prose-slate max-w-none">
        {@html $page.data.content}
    </article>
{/if}
```

## State Management

### Navigation Store

```typescript
// src/stores/navigation.ts
import { writable } from 'svelte/store';

interface NavigationState {
    tree: NavigationTree | null;
    loading: boolean;
    error: string | null;
    collapsed: Set<string>;  // Paths of collapsed items
}

function createNavigationStore() {
    const { subscribe, set, update } = writable<NavigationState>({
        tree: null,
        loading: true,
        error: null,
        collapsed: new Set(),
    });

    return {
        subscribe,
        load: async () => { /* fetch and set */ },
        toggle: (path: string) => { /* toggle collapsed state */ },
        expandTo: (path: string) => { /* expand all parents of path */ },
    };
}

export const navigation = createNavigationStore();
```

### Page Store

```typescript
// src/stores/page.ts
import { writable } from 'svelte/store';

interface PageState {
    data: PageResponse | null;
    loading: boolean;
    error: string | null;
}

function createPageStore() {
    const { subscribe, set, update } = writable<PageState>({
        data: null,
        loading: true,
        error: null,
    });

    return {
        subscribe,
        load: async (path: string) => { /* fetch and set */ },
    };
}

export const pageStore = createPageStore();
```

## Backend Integration

### Static File Serving

The Docstage backend needs to serve the built SPA files. Add to `docstage.server`:

```python
# Static file serving for SPA
app.router.add_static('/assets', static_dir / 'assets')

# SPA fallback - serve index.html for all non-API routes
async def spa_fallback(request: web.Request) -> web.Response:
    return web.FileResponse(static_dir / 'index.html')

# Add fallback route after API routes
app.router.add_get('/{path:.*}', spa_fallback)
```

### Configuration Extension

```toml
# docstage.toml
[frontend]
static_dir = "./frontend/dist"
```

## Build & Development

### Development Workflow

```bash
# Terminal 1: Backend
cd /path/to/docstage
uv run docstage serve --source-dir ./docs

# Terminal 2: Frontend (with proxy to backend)
cd frontend
npm run dev
```

### Vite Configuration

```typescript
// vite.config.ts
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
    plugins: [svelte()],
    server: {
        proxy: {
            '/api': 'http://localhost:8080',
        },
    },
});
```

### Production Build

```bash
cd frontend
npm run build  # Outputs to dist/
```

## Implementation Plan

### ~~Phase 1: Project Setup~~

1. ~~Initialize Vite + Svelte project with TypeScript.~~
2. ~~Implement native router using History API (no external library).~~
3. ~~Configure Tailwind CSS with Typography plugin.~~
4. ~~Create base layout structure.~~
5. ~~Set up API client with TypeScript interfaces.~~

### ~~Phase 2: Navigation~~

1. ~~Implement `NavigationSidebar` with tree rendering.~~
2. ~~Add collapse/expand functionality.~~
3. ~~Implement active state highlighting.~~
4. ~~Add localStorage persistence for collapse state.~~
5. ~~Implement mobile responsive drawer.~~

### Phase 3: Page Rendering

1. ~~Implement `PageContent` component.~~
2. ~~Add `Breadcrumbs` component.~~
3. ~~Configure Tailwind Typography for prose styling.~~
4. ~~Handle internal link interception for SPA navigation.~~

### Phase 4: Table of Contents

1. ~~Implement `TocSidebar` component.~~
2. Add scroll spy with IntersectionObserver.
3. ~~Implement smooth scroll to heading on click.~~
4. ~~Handle ToC visibility (hide when empty or on mobile).~~

### Phase 5: Backend Integration

1. Add static file serving to `docstage.server`.
2. Implement SPA fallback route.
3. Add frontend configuration to config schema.
4. Update CLI `serve` command to serve frontend.

### Phase 6: Polish

1. ~~Add loading states and skeleton screens.~~
2. ~~Implement error pages (404, network error).~~
3. ~~Add page title updates (`<title>` tag).~~
4. Performance optimization (lazy loading, caching).

## Dependencies

### Frontend (npm)

- `svelte` - Svelte 5 compiler
- `vite` - Build tool
- `@sveltejs/vite-plugin-svelte` - Vite plugin for Svelte
- `typescript` - Type checking
- `svelte-check` - Svelte type checking
- `tailwindcss` - Utility CSS
- `@tailwindcss/typography` - Prose styling
- `postcss` - CSS processing
- `autoprefixer` - CSS vendor prefixes

No external routing library - uses native History API.

### Backend (Python)

- `aiohttp-cors` - CORS middleware (development only)

## Success Metrics

1. **First Contentful Paint:** < 1 second.
2. **Time to Interactive:** < 2 seconds.
3. **Lighthouse Performance Score:** > 90.
4. **Bundle Size:** < 100KB gzipped (excluding content).

## Future Extensions

These features are explicitly deferred:

1. **Live Reload** - WebSocket integration for dev mode (Phase 5 of RD-001).
2. **Dark Mode** - Theme switching with localStorage persistence.
3. **Search** - Client-side search with pre-built index.
4. **Edit Links** - "Edit on GitHub" links for each page.
5. **Version Selector** - Switch between documentation versions.

## Design Decisions

1. **No SSR:** Client-side rendering only. SEO is not a concern for internal
   documentation.

2. **No authentication:** Docstage is auth-agnostic. Wrapper applications (e.g.,
   Backstage plugin) handle authentication at their layer.

## References

- [Stripe Documentation](https://docs.stripe.com/) - Design inspiration
- [Svelte 5 Documentation](https://svelte.dev/docs)
- [The Last SPA Router You'll Need](https://snugug.com/musings/the-last-spa-router-you-ll-need/) - Native routing approach
- [Vite](https://vitejs.dev/)
- [Tailwind CSS](https://tailwindcss.com/docs)
- [Tailwind Typography](https://tailwindcss.com/docs/typography-plugin)
- [RD-001: Docstage Backend](RD-001-docstage-backend.md)

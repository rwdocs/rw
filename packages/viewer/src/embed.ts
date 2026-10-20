// The `.js` below is deliberate, unlike the extensionless imports elsewhere:
// TypeScript copies a specifier verbatim into the emitted `.d.ts`, and `node16`
// resolution needs the extension or a consumer resolves a *directory* and gets
// no types. Only the graph reachable from this entry point needs it, and
// `npm run check:pack` enforces it.
import "./app.css";
import App from "./App.svelte";
import { mount, unmount, type ComponentProps } from "svelte";
import { on } from "svelte/events";
import type { NotifyFn } from "./types/notify.js";
import type { CommentApiClient } from "./api/comments.js";

export interface MountOptions {
  /** API base URL (host-supplied — e.g. `/api/rw` when proxied by a Backstage
   *  plugin to the rwdocs backend). */
  apiBaseUrl: string;
  /** Run in embedded mode (no pushState). Defaults to true. */
  embedded?: boolean;
  /** Initial path, relative to the viewer's base URL (the URL `resolveSectionRefs`
   *  maps `sectionRef` to), URL-encoded, with an optional hash: `/usecases/get-cards#errors`. */
  initialPath?: string;
  /** Section ref for this entity (e.g. "system:default/payment-gateway").
   *  Used to load scoped navigation and to resolve the viewer's own base URL via `resolveSectionRefs`. */
  sectionRef: string;
  /** Custom fetch function (e.g. Backstage authenticated fetch). */
  fetchFn?: typeof fetch;
  /** Called when the user navigates (embedded mode only).
   *  Receives the full href — for same-section navigation this is basePath + internal path,
   *  for cross-section navigation this is the resolved catalog URL.
   *  Use this to drive React Router or equivalent in the host application. */
  onNavigate?: (href: string) => void;
  /** Color scheme: 'light', 'dark', or 'auto' (OS preference). Defaults to 'auto'. */
  colorScheme?: "light" | "dark" | "auto";
  /** Resolve section refs to base URLs for cross-entity navigation.
   *  Called at startup with the viewer's own `sectionRef` to set the base URL, and after
   *  each page render with unique ref strings (e.g., "system:default/payment-gateway").
   *  Returns a map of ref → base URL. */
  resolveSectionRefs?: (refs: string[]) => Promise<Record<string, string>>;
  /** Surface a transient notification (e.g. a failed comment save) through the
   *  host's own toast/alert system. When omitted, the viewer renders its own
   *  built-in toaster. Intent is one of `info`/`success`/`warning`/`error`. */
  onNotify?: NotifyFn;
  /** Host-supplied comment client. When present, comments are enabled and all
   *  reads/writes (and optional live refresh) route through it — any URL shape or
   *  transport, decoupled from `apiBaseUrl`. When absent, the viewer builds the
   *  default HTTP client and reads `commentsEnabled` from `/config`.
   *
   *  Security: the host owns `bodyHtml` sanitization — see {@link CommentApiClient}. */
  comments?: CommentApiClient;
}

export interface RwInstance {
  /** Unmount the RW app and clean up. */
  destroy: () => void;
  /** Navigate to a path programmatically (for external navigation like browser back/forward).
   *  Takes the same base-relative form as `initialPath`, and calls `onNavigate`. */
  navigateTo: (path: string) => void;
  /** Update the color scheme without re-mounting. */
  setColorScheme: (scheme: "light" | "dark" | "auto") => void;
}

/**
 * Mount the RW documentation viewer into a DOM element.
 * Returns a handle with `destroy()`, `navigateTo()`, and `setColorScheme()` methods.
 */
export function mountRw(target: HTMLElement, options: MountOptions): RwInstance {
  target.setAttribute("data-rw-viewer", "");

  const applyDarkClass = (isDark: boolean) => {
    target.classList.toggle("dark", isDark);
  };

  let offMediaQuery: (() => void) | undefined;

  function applyColorScheme(scheme: "light" | "dark" | "auto") {
    offMediaQuery?.();
    offMediaQuery = undefined;

    if (scheme === "dark") {
      applyDarkClass(true);
    } else if (scheme === "light") {
      applyDarkClass(false);
    } else {
      const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
      applyDarkClass(mediaQuery.matches);
      offMediaQuery = on(mediaQuery, "change", (e) => applyDarkClass(e.matches));
    }
  }

  applyColorScheme(options.colorScheme ?? "auto");

  // ESLint and vite-plugin-dts see App through Svelte's `*.svelte` shim, whose
  // exports are untyped; svelte-check still holds these to App's real ones.
  const instance = mount<ComponentProps<typeof App>, { goto: (path: string) => void }>(App, {
    target,
    props: {
      apiBaseUrl: options.apiBaseUrl,
      embedded: options.embedded ?? true,
      initialPath: options.initialPath,
      sectionRef: options.sectionRef,
      fetchFn: options.fetchFn,
      onNavigate: options.onNavigate,
      resolveSectionRefs: options.resolveSectionRefs,
      onNotify: options.onNotify,
      comments: options.comments,
    },
  });

  return {
    destroy: () => {
      offMediaQuery?.();
      void unmount(instance);
      target.removeAttribute("data-rw-viewer");
      target.classList.remove("dark");
    },
    navigateTo: (path: string) => instance.goto(path),
    setColorScheme: (scheme: "light" | "dark" | "auto") => applyColorScheme(scheme),
  };
}

export type { CommentApiClient } from "./api/comments.js";
export type {
  Comment,
  CreateCommentRequest,
  UpdateCommentRequest,
  Author,
  Selector,
  CommentStatus,
} from "./types/comments.js";

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
  /** The host URL path that shows `sectionRef`'s root page, URL-encoded as
   *  `location.pathname` shows it: `/catalog/default/system/payment-gateway/docs`.
   *  `""` when the section's root is the origin's `/`. A trailing slash is ignored. */
  basePath: string;
  /** The host's location to open, as the browser shows it: `location.pathname
   *  + location.search + location.hash`, still URL-encoded. Outside `basePath`,
   *  the viewer opens the section's root page. */
  location: string;
  /** Section ref for this entity (e.g. "system:default/payment-gateway").
   *  Used to load scoped navigation. */
  sectionRef: string;
  /** Custom fetch function (e.g. Backstage authenticated fetch). */
  fetchFn?: typeof fetch;
  /** Called when the user navigates (embedded mode only).
   *  Receives the full href — for same-section navigation this is basePath + internal path,
   *  for cross-section navigation this is the resolved catalog URL.
   *  Use this to drive React Router or equivalent in the host application.
   *  Not called for {@link RwInstance.navigateTo}: the host already knows where it sent the viewer. */
  onNavigate?: (href: string) => void;
  /** Color scheme: 'light', 'dark', or 'auto' (OS preference). Defaults to 'auto'. */
  colorScheme?: "light" | "dark" | "auto";
  /** Resolve section refs to base URLs for cross-entity navigation.
   *  Called with the unique ref strings each navigation tree and page links to
   *  (e.g., "system:default/payment-gateway"). Returns a map of ref → base URL. */
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
  /** Show the host's new location. Call it on every location change, Back/Forward
   *  and hash-only changes included. Takes the same form as
   *  {@link MountOptions.location}, ignores a location outside `basePath`, and
   *  does not call `onNavigate`. */
  navigateTo: (location: string) => void;
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
      embedded: true,
      basePath: options.basePath,
      location: options.location,
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
    navigateTo: (location: string) => instance.goto(location),
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

import "./app.css";
import App from "./App.svelte";
import { mount, unmount } from "svelte";

export interface MountOptions {
  /** API base URL (e.g. "/api/rw"). */
  apiBaseUrl: string;
  /** Run in embedded mode (no pushState). Defaults to true. */
  embedded?: boolean;
  /** Initial path to navigate to. */
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
}

export interface RwInstance {
  /** Unmount the RW app and clean up. */
  destroy: () => void;
  /** Navigate to a path programmatically (for external navigation like browser back/forward). */
  navigateTo: (path: string) => void;
  /** Update the color scheme without re-mounting. */
  setColorScheme: (scheme: "light" | "dark" | "auto") => void;
}

/**
 * Mount the RW documentation viewer into a DOM element.
 * Returns a handle with `destroy()`, `navigateTo()`, and `setColorScheme()` methods.
 */
export function mountRw(target: HTMLElement, options: MountOptions): RwInstance {
  let gotoFn: ((path: string) => void) | undefined;

  target.setAttribute("data-rw-viewer", "");

  const applyDarkClass = (isDark: boolean) => {
    target.classList.toggle("dark", isDark);
  };

  let mediaQuery: MediaQueryList | undefined;
  let mediaQueryHandler: ((e: MediaQueryListEvent) => void) | undefined;

  function applyColorScheme(scheme: "light" | "dark" | "auto") {
    // Tear down any existing media query listener
    if (mediaQuery && mediaQueryHandler) {
      mediaQuery.removeEventListener("change", mediaQueryHandler);
      mediaQuery = undefined;
      mediaQueryHandler = undefined;
    }

    if (scheme === "dark") {
      applyDarkClass(true);
    } else if (scheme === "light") {
      applyDarkClass(false);
    } else {
      mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
      mediaQueryHandler = (e) => applyDarkClass(e.matches);
      applyDarkClass(mediaQuery.matches);
      mediaQuery.addEventListener("change", mediaQueryHandler);
    }
  }

  applyColorScheme(options.colorScheme ?? "auto");

  const instance = mount(App, {
    target,
    props: {
      apiBaseUrl: options.apiBaseUrl,
      embedded: options.embedded ?? true,
      initialPath: options.initialPath,
      sectionRef: options.sectionRef,
      fetchFn: options.fetchFn,
      onNavigate: options.onNavigate,
      resolveSectionRefs: options.resolveSectionRefs,
      exposeGoto: (goto: (path: string) => void) => {
        gotoFn = goto;
      },
    },
  });

  return {
    destroy: () => {
      if (mediaQuery && mediaQueryHandler) {
        mediaQuery.removeEventListener("change", mediaQueryHandler);
      }
      unmount(instance);
      target.removeAttribute("data-rw-viewer");
    },
    navigateTo: (path: string) => gotoFn?.(path),
    setColorScheme: (scheme: "light" | "dark" | "auto") => applyColorScheme(scheme),
  };
}

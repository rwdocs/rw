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
  /** Path prefix for link hrefs (e.g. "/rw-docs"). Links will use this prefix so that
   *  Cmd+Click, right-click → Open in new tab, and hover previews show correct URLs. */
  basePath?: string;
  /** Custom fetch function (e.g. Backstage authenticated fetch). */
  fetchFn?: typeof fetch;
  /** Called when the user navigates to a new path (embedded mode only). */
  onNavigate?: (path: string) => void;
}

export interface RwInstance {
  /** Unmount the RW app and clean up. */
  destroy: () => void;
  /** Navigate to a path programmatically (for external navigation like browser back/forward). */
  navigateTo: (path: string) => void;
}

/**
 * Mount the RW documentation viewer into a DOM element.
 * Returns a handle with `destroy()` and `navigateTo()` methods.
 */
export function mountRw(target: HTMLElement, options: MountOptions): RwInstance {
  let gotoFn: ((path: string) => void) | undefined;

  const instance = mount(App, {
    target,
    props: {
      apiBaseUrl: options.apiBaseUrl,
      embedded: options.embedded ?? true,
      initialPath: options.initialPath,
      basePath: options.basePath,
      fetchFn: options.fetchFn,
      onNavigate: options.onNavigate,
      exposeGoto: (goto: (path: string) => void) => {
        gotoFn = goto;
      },
    },
  });

  return {
    destroy: () => unmount(instance),
    navigateTo: (path: string) => gotoFn?.(path),
  };
}

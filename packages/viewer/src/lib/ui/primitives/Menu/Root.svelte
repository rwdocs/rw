<script lang="ts">
  import type { Snippet } from "svelte";
  import Popover from "../Popover.svelte";
  import { menuContext } from "./context";

  interface Props {
    /** Whether the menu is open. Use `bind:open` for two-way control. */
    open: boolean;
    /**
     * External element the menu anchors to. Menu is anchor-only — for
     * inline-trigger or free-coordinate overlays, use `Popover` directly.
     *
     * Menu.Root owns the trigger's ARIA contract while this element is
     * set: it writes `aria-haspopup="menu"`, `aria-controls`, and
     * `aria-expanded`, and listens for ArrowDown / ArrowUp to open the
     * menu with focus on the first / last item respectively. Consumers
     * are still responsible for wiring click/Enter/Space (e.g. via
     * `onclick={toggle}`) — native `<button>` handles Enter/Space by
     * synthesizing click.
     */
    anchorEl: HTMLElement | null;
    /** Which side of the anchor the menu sits on. */
    placement?: "top" | "bottom" | "left" | "right";
    /** Cross-axis alignment — see Popover. */
    align?: "start" | "end";
    /** Gap between anchor and menu panel, in px. */
    offset?: number;
    /**
     * Accessible name for the menu. Callers that trigger the menu from a
     * button with visible text can omit this; screen readers will fall back
     * to reading the trigger's aria-haspopup relationship.
     */
    "aria-label"?: string;
    children: Snippet;
    class?: string;
  }

  let {
    open = $bindable(),
    anchorEl,
    placement = "bottom",
    align = "start",
    offset = 4,
    "aria-label": ariaLabel,
    children,
    class: extraClass = "",
  }: Props = $props();

  let menuEl: HTMLElement | undefined = $state();

  // Roving tabindex anchor: the one menuitem that's tabbable from outside.
  // Disabled items never become active.
  let activeEl: HTMLElement | null = $state(null);

  const menuId = $props.id();

  // Signals which end of the item list to focus when the panel next mounts.
  // Set by the trigger's ArrowUp handler ("last") and consumed once by the
  // auto-focus effect.
  let pendingInitialFocus: "first" | "last" = "first";

  menuContext.set({
    close: () => {
      // Restore focus to the anchor before closing. Popover handles this
      // itself for Escape (via its own restoreFocusEl); for item activation
      // we'd otherwise leak focus to <body> once the panel unmounts. Anchor
      // focus is safe even when the consumer navigates on activation — the
      // new page takes focus a tick later.
      anchorEl?.focus();
      open = false;
    },
    isTabbable: (el) => el !== undefined && el === activeEl,
  });

  function enabledItems(): HTMLElement[] {
    if (!menuEl) return [];
    return Array.from(
      menuEl.querySelectorAll<HTMLElement>('[role="menuitem"]:not([aria-disabled="true"])'),
    );
  }

  function moveFocus(step: 1 | -1 | "first" | "last") {
    const items = enabledItems();
    if (items.length === 0) return;
    let nextIndex: number;
    if (step === "first") {
      nextIndex = 0;
    } else if (step === "last") {
      nextIndex = items.length - 1;
    } else {
      const currentIndex = items.indexOf(document.activeElement as HTMLElement);
      if (currentIndex === -1) {
        nextIndex = step === 1 ? 0 : items.length - 1;
      } else {
        nextIndex = (currentIndex + step + items.length) % items.length;
      }
    }
    items[nextIndex]?.focus();
  }

  // WAI-ARIA menu-button pattern: ArrowDown opens the menu focused on the
  // first item, ArrowUp opens it focused on the last. Enter/Space fall
  // through to the native button's click synthesis.
  function onTriggerKeydown(event: KeyboardEvent) {
    if (open) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      pendingInitialFocus = "first";
      open = true;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      pendingInitialFocus = "last";
      open = true;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        moveFocus(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveFocus(-1);
        break;
      case "Home":
        event.preventDefault();
        moveFocus("first");
        break;
      case "End":
        event.preventDefault();
        moveFocus("last");
        break;
      case "Tab":
        // WAI-ARIA menu pattern: Tab dismisses the menu. Restore focus to
        // the anchor first, then let the browser's default Tab action
        // advance focus from the trigger's position in the tab order — so
        // Tab moves to the element after the trigger and Shift+Tab to the
        // one before. Without this, Tab walks through interior menuitems
        // and leaves the menu open in the background.
        anchorEl?.focus();
        open = false;
        break;
    }
  }

  $effect(() => {
    if (!open || !menuEl) {
      activeEl = null;
      pendingInitialFocus = "first";
      return;
    }
    const items = menuEl.querySelectorAll<HTMLElement>(
      '[role="menuitem"]:not([aria-disabled="true"])',
    );
    if (items.length === 0) return;
    const target = pendingInitialFocus === "last" ? items[items.length - 1] : items[0];
    pendingInitialFocus = "first";
    activeEl = target;
    target.focus();
  });

  // Track focus moves between menuitems so the tab-stop follows the user's
  // active selection. Filter out disabled items — otherwise a click on a
  // disabled entry (whose click is preventDefault'd but still focuses the
  // element) would promote it to the sole tab stop.
  function onFocusIn(event: FocusEvent) {
    const target = event.target as HTMLElement | null;
    if (
      target?.getAttribute("role") === "menuitem" &&
      target.getAttribute("aria-disabled") !== "true"
    ) {
      activeEl = target;
    }
  }

  // Trigger ARIA + ArrowDown/ArrowUp wiring. Split into two effects so the
  // keydown listener only installs/removes on anchor change, not on every
  // open/close flip — `aria-expanded` is a write-only tracking effect that
  // doesn't need its own teardown (the identity effect cleans up the attr).
  $effect(() => {
    if (!anchorEl) return;
    const el = anchorEl;
    el.setAttribute("aria-haspopup", "menu");
    el.setAttribute("aria-controls", menuId);
    el.addEventListener("keydown", onTriggerKeydown);
    return () => {
      el.removeAttribute("aria-haspopup");
      el.removeAttribute("aria-controls");
      el.removeAttribute("aria-expanded");
      el.removeEventListener("keydown", onTriggerKeydown);
    };
  });

  $effect(() => {
    anchorEl?.setAttribute("aria-expanded", String(open));
  });
</script>

<Popover
  bind:open
  {anchorEl}
  {placement}
  {align}
  {offset}
  dismissible
  class="
    min-w-48 overflow-y-auto rounded-md border border-border-default bg-bg-raised py-1 shadow-lg
    {extraClass}
  "
>
  <!--
    tabindex="-1" keeps the container focusable programmatically (required by
    the a11y lint for interactive roles) without inserting it into the tab
    order; items drive their own focus via arrow keys and the auto-focus
    effect above.
  -->
  <div
    bind:this={menuEl}
    id={menuId}
    role="menu"
    aria-label={ariaLabel}
    tabindex="-1"
    onkeydown={onKeydown}
    onfocusin={onFocusIn}
  >
    {@render children()}
  </div>
</Popover>

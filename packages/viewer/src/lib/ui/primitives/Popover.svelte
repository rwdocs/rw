<script lang="ts">
  import type { Snippet } from "svelte";
  import { useAnchorOffset } from "../hooks/useAnchorOffset.svelte";

  type Placement = "top" | "bottom" | "left" | "right";
  type Align = "start" | "end";
  type PanelRole = "menu" | "listbox" | "tree" | "grid" | "dialog" | "tooltip";
  type PopupRole = Exclude<PanelRole, "tooltip">;

  interface Props {
    /** Whether the panel is rendered. Use `bind:open` for two-way control. */
    open: boolean;
    /** Which side of the anchor the panel sits on. Ignored in free mode. */
    placement?: Placement;
    /**
     * Cross-axis alignment. For top/bottom placements, `end` right-aligns the
     * panel's trailing edge with the anchor's trailing edge; for left/right
     * placements, `end` bottom-aligns. Ignored in free mode. Default `start`.
     */
    align?: Align;
    /** Gap between anchor and panel, in px. Ignored in free mode. */
    offset?: number;
    /** External anchor element. Mutually exclusive with `trigger` and x/y. */
    anchorEl?: HTMLElement | null;
    /** Free-mode x coordinate (viewport-fixed). Must pair with `y`. */
    x?: number;
    /** Free-mode y coordinate (viewport-fixed). Must pair with `x`. */
    y?: number;
    /** Dismiss the panel on Escape or outside-click. Requires `bind:open`. */
    dismissible?: boolean;
    /**
     * ARIA role for the panel. Omit for a generic floating container; compound
     * primitives built on Popover (e.g. Menu) pass `"menu"`, `"listbox"`, etc.
     */
    role?: PanelRole;
    /**
     * Inline anchor snippet. Wraps in an inline-block so layout survives.
     * Receives `{ controlProps }` — spread it on the interactive element to
     * wire ARIA relationship attributes correctly.
     */
    trigger?: Snippet<[{ controlProps: AriaControlProps }]>;
    children: Snippet;
    class?: string;
  }

  type AriaControlProps = {
    "aria-controls"?: string;
    "aria-expanded"?: boolean;
    "aria-haspopup"?: PopupRole;
    "aria-describedby"?: string;
  };

  let {
    open = $bindable(),
    placement = "bottom",
    align = "start",
    offset = 4,
    anchorEl = null,
    x,
    y,
    dismissible = false,
    role,
    trigger,
    children,
    class: extraClass = "",
  }: Props = $props();

  let triggerWrapperEl: HTMLElement | undefined = $state();
  let panelEl: HTMLElement | undefined = $state();

  const panelId = $props.id();

  // Tooltip semantics diverge: tooltips point from the described element via
  // `aria-describedby` and do NOT get `aria-expanded`/`aria-haspopup` because
  // they aren't disclosures. Everything else gets the standard disclosure set.
  const controlProps = $derived.by<AriaControlProps>(() => {
    if (role === "tooltip") {
      return { "aria-describedby": panelId };
    }
    const base: AriaControlProps = {
      "aria-controls": panelId,
      "aria-expanded": open,
    };
    if (role) {
      base["aria-haspopup"] = role;
    }
    return base;
  });

  const hasTrigger = $derived(trigger != null);
  const hasAnchorEl = $derived(anchorEl != null);
  const hasFreeCoords = $derived(x !== undefined && y !== undefined);

  // Validate the mode combination early. Throwing inside a $derived surfaces
  // the error the first time any reactive read touches it — in our case, the
  // positionStyle computation below, which runs on mount.
  const mode = $derived.by<"trigger" | "external" | "free">(() => {
    const count = Number(hasTrigger) + Number(hasAnchorEl) + Number(hasFreeCoords);
    if (count === 0) {
      if (x !== undefined || y !== undefined) {
        throw new Error("Popover: `x` and `y` must be provided together");
      }
      throw new Error("Popover: specify one of `trigger`, `anchorEl`, or (`x`, `y`)");
    }
    if (count > 1) {
      throw new Error("Popover: `trigger`, `anchorEl`, and (`x`, `y`) are mutually exclusive");
    }
    return hasTrigger ? "trigger" : hasAnchorEl ? "external" : "free";
  });

  // Only subscribe while open: trigger-mode Popovers (e.g. an always-mounted
  // TocPopover) would otherwise attach a capture-phase scroll listener for
  // every page instance and re-measure on every scroll tick with nothing to
  // display. Null in free mode is the hook's no-subscription signal.
  const anchorRect = useAnchorOffset(() => {
    if (!open) return null;
    if (mode === "trigger") return triggerWrapperEl ?? null;
    if (mode === "external") return anchorEl;
    return null;
  });

  // Fixed-position coordinates for the panel. Placement transforms avoid the
  // need to measure the panel itself — `translateY(-100%)` shifts by the
  // panel's own height at layout time, same for the horizontal version.
  //
  // Two axes: placement chooses which side of the anchor the panel sits on
  // (main axis), align chooses which edge of the anchor the panel lines up
  // against on the perpendicular axis (cross axis).
  const positionStyle = $derived.by(() => {
    if (mode === "free") {
      return `top: ${y}px; left: ${x}px;`;
    }
    const r = anchorRect;
    const transforms: string[] = [];
    let top = r.top;
    let left = r.left;

    switch (placement) {
      case "top":
        top = r.top - offset;
        transforms.push("translateY(-100%)");
        break;
      case "left":
        left = r.left - offset;
        transforms.push("translateX(-100%)");
        break;
      case "right":
        left = r.left + r.width + offset;
        break;
      case "bottom":
        top = r.top + r.height + offset;
        break;
    }

    const horizontalPlacement = placement === "left" || placement === "right";
    if (align === "end") {
      if (horizontalPlacement) {
        top = r.top + r.height;
        transforms.push("translateY(-100%)");
      } else {
        left = r.left + r.width;
        transforms.push("translateX(-100%)");
      }
    }

    const tx = transforms.length ? ` transform: ${transforms.join(" ")};` : "";
    return `top: ${top}px; left: ${left}px;${tx}`;
  });

  // True once the panel has coordinates to render at. Free mode is always
  // positioned (caller provides x/y); anchored modes wait for the first
  // getBoundingClientRect. While unpositioned, the panel renders with
  // `visibility: hidden` so it occupies no pixels and sits outside the a11y
  // tree, avoiding a one-frame flash at the viewport's top-left corner.
  const isPositioned = $derived(mode === "free" || anchorRect.measured);

  // Element that had focus when the panel last opened. Escape dismiss restores
  // focus here so keyboard users don't get dumped on <body>. Outside-click
  // dismiss deliberately does not restore — the user clicked somewhere else
  // and probably wants focus to follow their click.
  let restoreFocusEl: HTMLElement | null = null;
  $effect(() => {
    if (open) {
      restoreFocusEl =
        document.activeElement instanceof HTMLElement ? document.activeElement : null;
    }
  });

  // Outside-click + Escape. Registers only while open && dismissible so a
  // closed Popover never holds document-level listeners. Capture-phase click
  // matches the `lib/ui/hooks/dismissible` helper so behavior stays
  // consistent across call sites that still use it.
  $effect(() => {
    if (!open || !dismissible) return;

    function onClick(event: MouseEvent) {
      const target = event.target as Node;
      if (panelEl?.contains(target)) return;
      if (triggerWrapperEl?.contains(target)) return;
      if (mode === "external" && anchorEl?.contains(target)) return;
      open = false;
    }

    function onKeydown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        open = false;
        restoreFocusEl?.focus();
      }
    }

    document.addEventListener("click", onClick, true);
    window.addEventListener("keydown", onKeydown);
    return () => {
      document.removeEventListener("click", onClick, true);
      window.removeEventListener("keydown", onKeydown);
    };
  });
</script>

{#if trigger}
  <span bind:this={triggerWrapperEl} class="inline-block">
    {@render trigger({ controlProps })}
  </span>
{/if}

{#if open}
  <div
    bind:this={panelEl}
    id={panelId}
    class="fixed z-dropdown {extraClass}"
    style="{positionStyle}{isPositioned ? '' : ' visibility: hidden; pointer-events: none;'}"
    {role}
  >
    {@render children()}
  </div>
{/if}

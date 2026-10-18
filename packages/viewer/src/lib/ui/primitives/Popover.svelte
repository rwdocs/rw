<script lang="ts">
  import type { Snippet } from "svelte";
  import { on } from "svelte/events";
  import { useAnchorOffset } from "../hooks/useAnchorOffset.svelte";

  type Align = "start" | "end";

  interface Props {
    /** Whether the panel is rendered. Use `bind:open` for two-way control. */
    open: boolean;
    /**
     * Horizontal alignment of the panel below the anchor: `start` lines up the
     * left edges, `end` the right edges.
     */
    align?: Align;
    /** Gap below the anchor, in px. */
    offset?: number;
    /** External anchor element. Mutually exclusive with `trigger`. */
    anchorEl?: HTMLElement | null;
    /** Dismiss the panel on Escape or outside-click. Requires `bind:open`. */
    dismissible?: boolean;
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
    "aria-controls": string;
    "aria-expanded": boolean;
  };

  let {
    open = $bindable(),
    align = "start",
    offset = 4,
    anchorEl = null,
    dismissible = false,
    trigger,
    children,
    class: extraClass = "",
  }: Props = $props();

  let triggerWrapperEl: HTMLElement | undefined = $state();

  const panelId = $props.id();

  const controlProps = $derived<AriaControlProps>({
    "aria-controls": panelId,
    "aria-expanded": open,
  });

  // Throwing inside a $derived surfaces the error on the first reactive read,
  // which happens once the panel opens.
  const anchor = $derived.by(() => {
    if ((trigger != null) === (anchorEl != null)) {
      throw new Error("Popover: specify exactly one of `trigger` or `anchorEl`");
    }
    return trigger ? (triggerWrapperEl ?? null) : anchorEl;
  });

  // Only subscribe while open: trigger-mode Popovers (e.g. an always-mounted
  // TocPopover) would otherwise attach a capture-phase scroll listener for
  // every page instance and re-measure on every scroll tick with nothing to
  // display.
  const anchorRect = useAnchorOffset(() => (open ? anchor : null));

  // Until the anchor is first measured the panel is `visibility: hidden`, so it
  // occupies no pixels and sits outside the a11y tree instead of flashing at
  // the viewport's top-left corner for a frame. `translateX(-100%)`
  // right-aligns by the panel's own width at layout time, so the panel never
  // needs measuring.
  const positionStyle = $derived.by(() => {
    const r = anchorRect;
    if (!r.measured) return "visibility: hidden; pointer-events: none;";
    const top = r.top + r.height + offset;
    if (align === "end") {
      return `top: ${top}px; left: ${r.left + r.width}px; transform: translateX(-100%);`;
    }
    return `top: ${top}px; left: ${r.left}px;`;
  });

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

  // Outside-click + Escape dismiss. Attached to the panel, so the
  // document-level listeners exist only while the panel is mounted (i.e. while
  // open) — a closed Popover never holds them.
  function dismissOnInteraction(panel: HTMLElement) {
    if (!dismissible) return;

    function onClick(event: MouseEvent) {
      const target = event.target as Node;
      if (panel.contains(target) || anchor?.contains(target)) return;
      open = false;
    }

    function onKeydown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        open = false;
        restoreFocusEl?.focus();
      }
    }

    const offClick = on(document, "click", onClick, { capture: true });
    const offKeydown = on(window, "keydown", onKeydown);
    return () => {
      offClick();
      offKeydown();
    };
  }
</script>

{#if trigger}
  <span bind:this={triggerWrapperEl} class="inline-block">
    {@render trigger({ controlProps })}
  </span>
{/if}

{#if open}
  <div
    {@attach dismissOnInteraction}
    id={panelId}
    class="fixed z-dropdown {extraClass}"
    style={positionStyle}
  >
    {@render children()}
  </div>
{/if}

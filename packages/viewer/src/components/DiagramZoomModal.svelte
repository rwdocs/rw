<script lang="ts">
  import { untrack } from "svelte";
  import IconButton from "$lib/ui/primitives/IconButton.svelte";
  import Button from "$lib/ui/primitives/Button.svelte";
  import ButtonGroup from "$lib/ui/primitives/ButtonGroup.svelte";
  import {
    initialViewBox,
    zoomViewBox,
    panViewBox,
    scaleOfViewBox,
    viewBoxSizeForScale,
    fitScale,
    clampScale,
    parseViewBox,
    MAX_SCALE,
    type ViewBox,
    type Size,
  } from "$lib/diagram/zoomMath";
  import { naturalSizeOf } from "$lib/diagram/naturalSize";
  import { namespaceIds } from "$lib/diagram/namespaceIds";

  interface Props {
    /**
     * Stable identity of the open diagram — its `data-diagram-id`. `null` closes the
     * popup. A change to a new non-null id is a *fresh open* (resets zoom to fit); the
     * same id with a new `figure` is a *live update* (preserves the current zoom/pan).
     */
    diagramId: string | null;
    /**
     * The article's current `<figure class="diagram">` for `diagramId`. `null` while
     * `diagramId` is set means the diagram is momentarily broken (e.g. a save with a
     * syntax error); the popup then keeps its last good render and the caller surfaces
     * the error out of band.
     */
    figure: HTMLElement | null;
    /** Called when the popup should close (Escape, Close button). */
    onClose: () => void;
  }

  let { diagramId, figure, onClose }: Props = $props();

  // Identity of the diagram currently rendered into the dialog, so the open/update
  // effect can tell a fresh open (reset to fit) from a live-reload swap (preserve
  // zoom). Plain: read only inside the effect, never in the template.
  let shownDiagramId: string | null = null;

  const MARGIN = 32;
  const BUTTON_STEP = 1.25;
  const SVG_NS = "http://www.w3.org/2000/svg";

  let dialogEl = $state<HTMLDialogElement>();
  let viewportEl = $state<HTMLElement>();
  let cloneHost = $state<HTMLElement>();

  // The current viewBox drives what slice of the diagram is shown. Writing it to
  // the SVG re-renders the vector paths crisply at the new zoom (a repaint, not a
  // layout reflow or a bitmap stretch — so it stays sharp AND smooth, unlike a
  // CSS `transform: scale()` which rasterizes once and scales the bitmap).
  let viewBox = $state<ViewBox>({ x: 0, y: 0, w: 1, h: 1 });

  // Frozen at open; plain (read only in handlers, never in the template):
  let svgClone: SVGSVGElement | undefined; // the SVG element we drive
  let diagram: ViewBox = { x: 0, y: 0, w: 1, h: 1 }; // its intrinsic viewBox (user units)
  let natural: Size = { w: 1, h: 1 }; // diagram px size at scale 1
  let viewport: Size = { w: 1, h: 1 }; // the SVG box size (fills the popup viewport)
  let minScale = 0.1; // the opening scale (100% or fit) — can't zoom out past it

  // Live zoom level (100% = natural size), shown in the toolbar. Recomputes when
  // the viewBox width changes (zoom); panning leaves the width alone, so it holds
  // steady while dragging. `diagram`/`natural`/`viewport` are plain, but they're
  // frozen for the session before `viewBox` is first assigned in reset().
  let zoomPercent = $derived(
    Math.round(scaleOfViewBox(diagram, natural, viewport, viewBox.w) * 100),
  );

  // Pan/pinch pointer bookkeeping. Panning is derived from `pointers.size === 1`,
  // so a 2->1 finger transition resumes panning and a 3rd finger can't corrupt an
  // in-progress pinch.
  const pointers = new Map<number, { x: number; y: number }>();
  let pinchStartDist = 0;
  let pinchStartVbW = 0; // viewBox width when the pinch began

  // Reflect the reactive viewBox onto the SVG element.
  $effect(() => {
    const v = viewBox;
    svgClone?.setAttribute("viewBox", `${v.x} ${v.y} ${v.w} ${v.h}`);
  });

  // Cached at open/reset/resize so the wheel and pinch hot paths don't force a
  // synchronous layout read (getBoundingClientRect) on every event. The dialog is
  // a fixed fullscreen top-layer element, so its rect only moves on a viewport
  // resize — which `remeasure()` handles.
  let viewportRect: DOMRect | undefined;

  function measureViewport(): Size {
    if (!viewportEl) return { w: 1, h: 1 };
    const r = viewportEl.getBoundingClientRect();
    viewportRect = r;
    return { w: Math.max(1, r.width), h: Math.max(1, r.height) };
  }

  function currentScale(): number {
    return scaleOfViewBox(diagram, natural, viewport, viewBox.w);
  }

  function reset() {
    viewport = measureViewport();
    // Open at 100% (or fit-down if larger than the popup); can't zoom out past
    // that opening scale, and can zoom in up to MAX_SCALE (8x natural).
    minScale = Math.min(1, fitScale(diagram, natural, viewport, MARGIN));
    viewBox = initialViewBox(diagram, natural, viewport, MARGIN);
  }

  // Re-measure the viewport and re-apply a target scale centered on (cx, cy),
  // recomputing `minScale` against the (possibly new) diagram/viewport. Shared by
  // the window-resize handler and the live-update path, which both need to keep a
  // chosen zoom/center steady while the geometry underneath them changes.
  function applyScaleCentered(scale: number, cx: number, cy: number) {
    viewport = measureViewport();
    minScale = Math.min(1, fitScale(diagram, natural, viewport, MARGIN));
    const { w, h } = viewBoxSizeForScale(
      diagram,
      natural,
      viewport,
      clampScale(scale, minScale, MAX_SCALE),
    );
    viewBox = { x: cx - w / 2, y: cy - h / 2, w, h };
  }

  // Re-measure after a window resize / device rotation while the popup is open,
  // preserving the current zoom and center. Without this, `viewport` (the divisor
  // in all the zoom/pan math) goes stale and zoom-to-cursor, pan, and the %
  // readout all misbehave until the user hits Reset.
  function remeasure() {
    applyScaleCentered(
      clampScale(currentScale(), minScale, MAX_SCALE),
      viewBox.x + viewBox.w / 2,
      viewBox.y + viewBox.h / 2,
    );
  }

  $effect(() => {
    if (!viewportEl) return;
    const onResize = () => remeasure();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  });

  function zoomAt(clientX: number, clientY: number, targetScale: number, rect?: DOMRect) {
    const r = rect ?? viewportRect ?? viewportEl?.getBoundingClientRect();
    if (!r) return;
    viewBox = zoomViewBox(
      viewBox,
      diagram,
      natural,
      viewport,
      clientX - r.left,
      clientY - r.top,
      targetScale,
      minScale,
      MAX_SCALE,
    );
  }

  function zoomFromButton(factor: number) {
    if (!viewportEl) return;
    const r = viewportEl.getBoundingClientRect();
    zoomAt(r.left + r.width / 2, r.top + r.height / 2, currentScale() * factor, r);
  }

  /** The intrinsic viewBox of an SVG, or a `0 0 w h` fallback from `natural`. */
  function readViewBox(svg: SVGSVGElement, fallback: Size): ViewBox {
    return (
      parseViewBox(svg.getAttribute("viewBox")) ?? { x: 0, y: 0, w: fallback.w, h: fallback.h }
    );
  }

  /** Normalize the source (inline SVG or PNG <img>) to an SVG driven via viewBox. */
  function buildSvg(source: SVGSVGElement | HTMLImageElement, nat: Size): SVGSVGElement {
    if (source instanceof HTMLImageElement) {
      const svg = document.createElementNS(SVG_NS, "svg");
      // Mark the raster wrapper so the dark-mode invert filter skips it — a PNG
      // diagram is left untouched in the article (only `svg` is inverted there),
      // and the popup must match rather than invert the bitmap.
      svg.setAttribute("data-raster", "");
      svg.setAttribute("viewBox", `0 0 ${nat.w} ${nat.h}`);
      const image = document.createElementNS(SVG_NS, "image");
      image.setAttribute("href", source.src);
      image.setAttribute("width", String(nat.w));
      image.setAttribute("height", String(nat.h));
      svg.appendChild(image);
      return svg;
    }
    // Deep clone duplicates every id while the original still lives in the
    // article; namespace them so the clone's url(#…)/href references resolve to
    // its own defs (not the original's, which a live reload can destroy).
    const clone = source.cloneNode(true) as SVGSVGElement;
    namespaceIds(clone);
    return clone;
  }

  // Open / live-update / close, driven by (diagramId, figure). Reading dialogEl and
  // cloneHost keeps the effect re-running as they bind; the body runs untracked so
  // reading viewBox/svgClone (to preserve zoom across an update) can't feed back and
  // re-run the effect on every zoom/pan. The close branch must not depend on
  // cloneHost/viewportEl — those unbind the moment the popup closes.
  $effect(() => {
    const id = diagramId;
    const fig = figure;
    const dlg = dialogEl;
    const host = cloneHost;
    if (!dlg) return;

    untrack(() => {
      if (id === null) {
        cloneHost?.replaceChildren();
        svgClone = undefined;
        shownDiagramId = null;
        if (dlg.open) {
          if (typeof dlg.close === "function") dlg.close();
          else dlg.removeAttribute("open");
        }
        return;
      }

      // Same id + new figure = live reload swapped this diagram's render; keep the
      // current zoom/center. A new id (or opening from closed) is a fresh open.
      const isFreshOpen = id !== shownDiagramId;

      if (fig && host) {
        const preserve =
          !isFreshOpen && svgClone
            ? {
                scale: currentScale(),
                cx: viewBox.x + viewBox.w / 2,
                cy: viewBox.y + viewBox.h / 2,
              }
            : null;

        // Start each session with clean gesture state: a prior session that closed
        // mid-gesture never received its pointerup (viewportEl was torn down first).
        pointers.clear();
        pinchStartDist = 0;

        // Direct children only: the injected `.diagram-expand-btn` lives inside the
        // same figure and carries its own icon <svg>, which an unscoped `svg, img`
        // would match on a figure whose diagram failed to produce any real svg/img.
        const source = fig.querySelector<SVGSVGElement | HTMLImageElement>(
          ":scope > svg, :scope > img",
        );
        host.replaceChildren();
        if (source) {
          natural = naturalSizeOf(source);
          const svg = buildSvg(source, natural);
          diagram =
            source instanceof HTMLImageElement
              ? { x: 0, y: 0, w: natural.w, h: natural.h }
              : readViewBox(svg, natural);
          // The SVG fills the viewport box; the viewBox (not the box size) zooms.
          // Force `meet` so Kroki's `preserveAspectRatio="none"` can't stretch the
          // diagram to the popup's aspect ratio (the viewBox already matches the
          // viewport aspect, so `meet` adds no letterbox).
          svg.setAttribute("preserveAspectRatio", "xMidYMid meet");
          svg.removeAttribute("width");
          svg.removeAttribute("height");
          // Kroki SVGs pin an intrinsic size via inline `style="width:NNpx;..."`,
          // which beats the stylesheet's `width:100%` and would leave the diagram
          // at its (small) natural size instead of filling the popup. Override the
          // inline properties directly so the SVG fills its box and the viewBox
          // controls the zoom.
          svg.style.setProperty("width", "100%");
          svg.style.setProperty("height", "100%");
          svg.style.setProperty("max-width", "none");
          svg.style.setProperty("max-height", "none");
          svgClone = svg;
          host.appendChild(svg);
        } else {
          natural = { w: 1, h: 1 };
          diagram = { x: 0, y: 0, w: 1, h: 1 };
          svgClone = undefined;
        }
        shownDiagramId = id;

        // Open first so the viewport has layout, then compute the initial fit.
        if (!dlg.open) {
          if (typeof dlg.showModal === "function") dlg.showModal();
          else dlg.setAttribute("open", "");
        }
        // Preserve the prior zoom/center on a live swap; fit-to-view on a fresh open.
        if (preserve) applyScaleCentered(preserve.scale, preserve.cx, preserve.cy);
        else reset();
      } else if (fig === null) {
        // Diagram momentarily broken (a save with a syntax error) while the popup
        // stays open: keep the last good clone and the current zoom untouched, and
        // just make sure the dialog is open. The caller surfaces the error.
        shownDiagramId = id;
        if (!dlg.open) {
          if (typeof dlg.showModal === "function") dlg.showModal();
          else dlg.setAttribute("open", "");
        }
      }
    });
  });

  // Non-passive wheel so we can preventDefault the page scroll while zooming.
  $effect(() => {
    const el = viewportEl;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
      zoomAt(e.clientX, e.clientY, currentScale() * factor);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  });

  function onPointerDown(e: PointerEvent) {
    if (!viewportEl) return;
    // Ignore secondary mouse buttons (right/middle): a context-menu click must
    // not start a pan. Touch and pen always report button 0.
    if (e.pointerType === "mouse" && e.button !== 0) return;
    viewportEl.setPointerCapture?.(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });

    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      pinchStartDist = Math.hypot(a.x - b.x, a.y - b.y);
      pinchStartVbW = viewBox.w;
    }
  }

  function onPointerMove(e: PointerEvent) {
    const prev = pointers.get(e.pointerId);
    if (!prev) return;
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });

    if (pointers.size === 2 && pinchStartDist > 0) {
      const [a, b] = [...pointers.values()];
      const dist = Math.hypot(a.x - b.x, a.y - b.y);
      const startScale = scaleOfViewBox(diagram, natural, viewport, pinchStartVbW);
      zoomAt((a.x + b.x) / 2, (a.y + b.y) / 2, startScale * (dist / pinchStartDist));
    } else if (pointers.size === 1) {
      const dx = e.clientX - prev.x;
      const dy = e.clientY - prev.y;
      viewBox = panViewBox(viewBox, viewport, dx, dy);
    }
  }

  function onPointerUp(e: PointerEvent) {
    pointers.delete(e.pointerId);
    if (pointers.size < 2) pinchStartDist = 0;
  }
</script>

<dialog
  bind:this={dialogEl}
  class="diagram-zoom-dialog"
  aria-label="Diagram viewer"
  oncancel={(e) => {
    e.preventDefault();
    onClose();
  }}
  onclose={() => {
    if (diagramId !== null) onClose();
  }}
>
  {#if diagramId !== null}
    <div class="diagram-zoom-toolbar">
      <!-- Zoom controls, segmented into one cluster. The centre readout shows the
           live zoom level and resets to fit when clicked. -->
      <ButtonGroup aria-label="Zoom controls">
        <IconButton aria-label="Zoom out" onclick={() => zoomFromButton(1 / BUTTON_STEP)}>
          <svg
            class="size-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"><path d="M5 12h14" /></svg
          >
        </IconButton>
        <Button
          variant="secondary"
          size="sm"
          aria-label="Reset zoom"
          title="Reset to fit"
          class="h-8 min-w-14 tabular-nums"
          onclick={reset}
        >
          {zoomPercent}%
        </Button>
        <IconButton aria-label="Zoom in" onclick={() => zoomFromButton(BUTTON_STEP)}>
          <svg
            class="size-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"><path d="M12 5v14M5 12h14" /></svg
          >
        </IconButton>
      </ButtonGroup>
      <!-- Close is a different class of action (dismiss the modal), set apart from
           the zoom cluster so it can't be fat-fingered while zooming. -->
      <IconButton aria-label="Close" onclick={onClose}>
        <svg
          class="size-4"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"><path d="M6 18 18 6M6 6l12 12" /></svg
        >
      </IconButton>
    </div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      bind:this={viewportEl}
      class="diagram-zoom-viewport"
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
    >
      <div
        bind:this={cloneHost}
        data-testid="diagram-zoom-content"
        class="diagram-zoom-content"
      ></div>
    </div>
  {/if}
</dialog>

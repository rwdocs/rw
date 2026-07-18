import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import { flushSync, tick } from "svelte";
import DiagramZoomModal from "./DiagramZoomModal.svelte";

// jsdom lacks dialog methods; stub them so the component's guards exercise the real path.
beforeEach(() => {
  if (!HTMLDialogElement.prototype.showModal) {
    HTMLDialogElement.prototype.showModal = function () {
      this.setAttribute("open", "");
    };
  }
  if (!HTMLDialogElement.prototype.close) {
    HTMLDialogElement.prototype.close = function () {
      this.removeAttribute("open");
      this.dispatchEvent(new Event("close"));
    };
  }
});

/** A `<figure class="diagram">` with a marker so a swapped clone is identifiable. */
function diagramFigure(marker = "a"): HTMLElement {
  const fig = document.createElement("figure");
  fig.className = "diagram";
  fig.innerHTML = `<svg viewBox="0 0 200 100" width="200" height="100" data-marker="${marker}"></svg>`;
  document.body.appendChild(fig);
  return fig;
}

const closed = { diagramId: null, figure: null, onClose: () => {} };

/** The light-DOM host the clone's shadow root hangs off. */
function contentHost(root: ParentNode): HTMLElement | null {
  return root.querySelector('[data-testid="diagram-zoom-content"]');
}

/**
 * The cloned diagram inside the popup's shadow root. The clone is mounted into
 * a shadow root (its own id scope, so it cannot collide with the original still
 * in the article), which light-DOM `querySelector` does not pierce.
 */
function clonedSvg(root: ParentNode, selector = "svg"): SVGSVGElement | null {
  return contentHost(root)?.shadowRoot?.querySelector<SVGSVGElement>(selector) ?? null;
}

describe("DiagramZoomModal", () => {
  it("renders nothing interactive until a diagram is set", () => {
    const { container } = render(DiagramZoomModal, { props: closed });
    const dialog = container.querySelector("dialog")!;
    expect(dialog.hasAttribute("open")).toBe(false);
    // Assert directly on the host's absence, not on `clonedSvg`'s optional
    // chaining: that would also read `null` if the host existed but its
    // shadow root were merely empty, which isn't the case being tested here.
    expect(contentHost(dialog)).toBeNull();
  });

  it("clones the diagram into the dialog when opened", async () => {
    const fig = diagramFigure();
    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: fig, onClose: () => {} });
    flushSync();
    await tick();
    const dialog = container.querySelector("dialog")!;
    expect(clonedSvg(dialog, "svg")).not.toBeNull();
    expect(dialog.getAttribute("aria-label")).toBe("Diagram viewer");
  });

  it("makes a Kroki-style SVG fill the viewport instead of its pinned intrinsic size", async () => {
    // Kroki SVGs pin an intrinsic size via inline style and set
    // preserveAspectRatio="none"; both would leave the diagram small/stretched
    // in the popup. The clone must override them.
    const fig = document.createElement("figure");
    fig.className = "diagram";
    fig.innerHTML =
      '<svg viewBox="0 0 268 541" width="268" height="541" preserveAspectRatio="none" style="width:268px;height:541px;"></svg>';
    document.body.appendChild(fig);

    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: fig, onClose: () => {} });
    flushSync();
    await tick();

    const svg = clonedSvg(container)!;
    expect(svg.style.width).toBe("100%");
    expect(svg.style.height).toBe("100%");
    expect(svg.getAttribute("preserveAspectRatio")).toBe("xMidYMid meet");
    // The intrinsic width/height attributes are dropped so CSS/viewBox drive size.
    expect(svg.hasAttribute("width")).toBe(false);
    expect(svg.hasAttribute("height")).toBe(false);
  });

  it("marks the host data-raster for a PNG <img> diagram so the popup does not invert it", async () => {
    // The article never inverts PNG diagrams (only `svg`), so the popup marks
    // the raster wrapper — and reflects the marker onto the light-DOM host,
    // which is where the theme-dependent invert rule has to live.
    const fig = document.createElement("figure");
    fig.className = "diagram";
    const img = document.createElement("img");
    img.setAttribute("src", "diagram.png");
    img.setAttribute("width", "300");
    img.setAttribute("height", "150");
    fig.appendChild(img);
    document.body.appendChild(fig);

    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: fig, onClose: () => {} });
    flushSync();
    await tick();

    const svg = clonedSvg(container)!;
    expect(svg.querySelector("image")).not.toBeNull();
    // The invert rule cannot see inside the shadow root, so the host carries it.
    expect(contentHost(container)!.hasAttribute("data-raster")).toBe(true);
  });

  it("leaves the raster marker off the host for a vector diagram", async () => {
    const fig = diagramFigure();
    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: fig, onClose: () => {} });
    flushSync();
    await tick();
    expect(contentHost(container)!.hasAttribute("data-raster")).toBe(false);
  });

  it("keeps the clone's ids intact in its own shadow scope", async () => {
    // The clone used to have every id rewritten, because clone and original
    // shared one document scope. Mounting into a shadow root separates the
    // scopes, so ids and the url(#…) references pointing at them survive as-is.
    const fig = document.createElement("figure");
    fig.className = "diagram";
    fig.innerHTML =
      '<svg viewBox="0 0 10 10"><defs><marker id="arrow"></marker></defs>' +
      '<path id="p" marker-end="url(#arrow)" /></svg>';
    document.body.appendChild(fig);

    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: fig, onClose: () => {} });
    flushSync();
    await tick();

    const svg = clonedSvg(container)!;
    expect(svg.querySelector("#arrow")).not.toBeNull();
    expect(svg.querySelector("path")!.getAttribute("marker-end")).toBe("url(#arrow)");
    // The original in the article is untouched by the clone sharing its ids.
    expect(fig.querySelector("#arrow")).not.toBeNull();
  });

  it("reads the source from a <rw-diagram> shadow root", async () => {
    // Server-rendered figures wrap the SVG in `<rw-diagram>`; the popup must
    // find the diagram there, not only as a direct child of the figure.
    const fig = document.createElement("figure");
    fig.className = "diagram";
    const wrapper = document.createElement("rw-diagram");
    wrapper.attachShadow({ mode: "open" }).innerHTML =
      '<svg viewBox="0 0 8 8" data-marker="wrapped"></svg>';
    fig.appendChild(wrapper);
    document.body.appendChild(fig);

    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: fig, onClose: () => {} });
    flushSync();
    await tick();

    expect(clonedSvg(container, 'svg[data-marker="wrapped"]')).not.toBeNull();
  });

  it("does not mistake the injected expand-button icon for the diagram source", async () => {
    // A figure whose diagram produced no svg/img still gets an expand button with
    // its own icon <svg>. Scoping to direct children keeps that icon from being
    // cloned and shown enlarged as the 'diagram'.
    const fig = document.createElement("figure");
    fig.className = "diagram";
    const btn = document.createElement("button");
    btn.className = "diagram-expand-btn";
    btn.innerHTML = '<svg viewBox="0 0 24 24"><path d="M0 0h24" /></svg>';
    fig.appendChild(btn);
    document.body.appendChild(fig);

    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: fig, onClose: () => {} });
    flushSync();
    await tick();

    // The host and its shadow root do exist (the popup is open with a real
    // figure) — assert the svg is absent from a live shadow root, not via
    // `clonedSvg`'s optional chaining, which would read `null` just the same
    // if the host itself were missing.
    const host = contentHost(container);
    expect(host).not.toBeNull();
    expect(host!.shadowRoot!.querySelector("svg")).toBeNull();
  });

  it("exposes zoom in/out/reset/close controls with labels", async () => {
    const fig = diagramFigure();
    const { getByLabelText, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: fig, onClose: () => {} });
    flushSync();
    expect(getByLabelText("Zoom in")).toBeTruthy();
    expect(getByLabelText("Zoom out")).toBeTruthy();
    expect(getByLabelText("Reset zoom")).toBeTruthy();
    expect(getByLabelText("Close")).toBeTruthy();
  });

  it("calls onClose when the close button is clicked", async () => {
    const fig = diagramFigure();
    const onClose = vi.fn();
    const { getByLabelText, rerender } = render(DiagramZoomModal, {
      props: { diagramId: null, figure: null, onClose },
    });
    await rerender({ diagramId: "d0", figure: fig, onClose });
    flushSync();
    (getByLabelText("Close") as HTMLElement).click();
    expect(onClose).toHaveBeenCalled();
  });

  it("closes the dialog and clears the clone when the diagram id is cleared", async () => {
    const fig = diagramFigure();
    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: fig, onClose: () => {} });
    flushSync();
    const dialog = container.querySelector("dialog")!;
    expect(dialog.hasAttribute("open")).toBe(true);
    // Capture the host now: the `{#if}` unmounts it on close regardless of
    // what `clearCloneRoot` does, so asserting only through the (now-detached)
    // DOM tree afterward wouldn't tell `clearCloneRoot` ran from the host
    // simply being gone. The captured element and its shadow root are still
    // live JS objects even once detached, so querying them after close proves
    // the clone content was actually removed.
    const host = contentHost(dialog)!;
    expect(host.shadowRoot!.querySelector("svg")).not.toBeNull();

    // Clearing the id (Escape/close/navigation) must close the dialog. The close
    // branch of the open/close effect must not depend on the inner nodes, which
    // unbind the moment the popup closes.
    await rerender({ diagramId: null, figure: null, onClose: () => {} });
    flushSync();
    expect(dialog.hasAttribute("open")).toBe(false);
    expect(contentHost(dialog)).toBeNull();
    expect(host.shadowRoot!.querySelector("svg")).toBeNull();
  });

  it("swaps in the new render when the same diagram live-updates", async () => {
    // A live reload re-resolves the same diagram id to a freshly-rendered figure.
    // Same id + new figure = update: the popup shows the new render's content.
    const first = diagramFigure("before");
    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: first, onClose: () => {} });
    flushSync();
    await tick();
    expect(clonedSvg(container, 'svg[data-marker="before"]')).not.toBeNull();

    const second = diagramFigure("after");
    await rerender({ diagramId: "d0", figure: second, onClose: () => {} });
    flushSync();
    await tick();
    expect(clonedSvg(container, 'svg[data-marker="after"]')).not.toBeNull();
    expect(clonedSvg(container, 'svg[data-marker="before"]')).toBeNull();
    // The dialog stays open across the swap.
    expect(container.querySelector("dialog")!.hasAttribute("open")).toBe(true);
  });

  it("keeps the last good render when the diagram is momentarily broken", async () => {
    // figure === null while the id stays set = the current source is broken (a
    // syntax error mid-edit). The popup must keep the last good clone, not blank out.
    const good = diagramFigure("good");
    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: good, onClose: () => {} });
    flushSync();
    await tick();

    await rerender({ diagramId: "d0", figure: null, onClose: () => {} });
    flushSync();
    await tick();

    const dialog = container.querySelector("dialog")!;
    expect(dialog.hasAttribute("open")).toBe(true);
    // Last good render is still on screen.
    expect(clonedSvg(dialog, 'svg[data-marker="good"]')).not.toBeNull();
  });

  it("re-opens for a different diagram after one was shown", async () => {
    const first = diagramFigure("one");
    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: first, onClose: () => {} });
    flushSync();
    await tick();

    // A different id is a fresh open (e.g. expanding another diagram): its content
    // replaces the first.
    const second = diagramFigure("two");
    await rerender({ diagramId: "d1", figure: second, onClose: () => {} });
    flushSync();
    await tick();
    expect(clonedSvg(container, 'svg[data-marker="two"]')).not.toBeNull();
    expect(clonedSvg(container, 'svg[data-marker="one"]')).toBeNull();
  });

  it("keeps the shadow root styled after switching to a different diagram without closing", async () => {
    // `ensureCloneRoot` short-circuits when the host is unchanged, so the sheet
    // `applySheet` installed at open is never re-applied on a diagram switch or
    // live update — `clearCloneRoot` must not remove it along with the old
    // clone. A close/reopen cycle would NOT catch a regression here: closing
    // tears the host down (`{#if diagramId !== null}`), so reopening attaches a
    // fresh shadow root with a fresh sheet regardless of how the old one was
    // cleared. Only a same-host switch (id changes, popup stays open)
    // exercises the short-circuit.
    const first = diagramFigure("one");
    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: first, onClose: () => {} });
    flushSync();
    await tick();

    const second = diagramFigure("two");
    await rerender({ diagramId: "d1", figure: second, onClose: () => {} });
    flushSync();
    await tick();

    const shadow = contentHost(container)!.shadowRoot!;
    // jsdom takes the `<style>` fallback path; a real browser takes the
    // adopted-stylesheet path (see `applySheet`) — check both so the pin holds
    // either way.
    const hasStyleEl = shadow.querySelector("style") !== null;
    const hasAdoptedSheet = (shadow.adoptedStyleSheets ?? []).length > 0;
    expect(hasStyleEl || hasAdoptedSheet).toBe(true);
  });
});

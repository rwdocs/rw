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

describe("DiagramZoomModal", () => {
  it("renders nothing interactive until a diagram is set", () => {
    const { container } = render(DiagramZoomModal, { props: closed });
    const dialog = container.querySelector("dialog")!;
    expect(dialog.hasAttribute("open")).toBe(false);
    expect(dialog.querySelector('[data-testid="diagram-zoom-content"] svg')).toBeNull();
  });

  it("clones the diagram into the dialog when opened", async () => {
    const fig = diagramFigure();
    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: fig, onClose: () => {} });
    flushSync();
    await tick();
    const dialog = container.querySelector("dialog")!;
    expect(dialog.querySelector('[data-testid="diagram-zoom-content"] svg')).not.toBeNull();
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

    const svg = container.querySelector<SVGSVGElement>('[data-testid="diagram-zoom-content"] svg')!;
    expect(svg.style.width).toBe("100%");
    expect(svg.style.height).toBe("100%");
    expect(svg.getAttribute("preserveAspectRatio")).toBe("xMidYMid meet");
    // The intrinsic width/height attributes are dropped so CSS/viewBox drive size.
    expect(svg.hasAttribute("width")).toBe(false);
    expect(svg.hasAttribute("height")).toBe(false);
  });

  it("wraps a PNG <img> diagram as a data-raster svg so the popup does not invert it", async () => {
    // The article never inverts PNG diagrams (only `svg`), so the popup marks the
    // raster wrapper and the dark-mode invert rule skips `svg[data-raster]`.
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

    const svg = container.querySelector<SVGSVGElement>('[data-testid="diagram-zoom-content"] svg')!;
    expect(svg.hasAttribute("data-raster")).toBe(true);
    expect(svg.querySelector("image")).not.toBeNull();
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

    expect(container.querySelector('[data-testid="diagram-zoom-content"] svg')).toBeNull();
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

    // Clearing the id (Escape/close/navigation) must close the dialog. The close
    // branch of the open/close effect must not depend on the inner nodes, which
    // unbind the moment the popup closes.
    await rerender({ diagramId: null, figure: null, onClose: () => {} });
    flushSync();
    expect(dialog.hasAttribute("open")).toBe(false);
    expect(dialog.querySelector('[data-testid="diagram-zoom-content"] svg')).toBeNull();
  });

  it("swaps in the new render when the same diagram live-updates", async () => {
    // A live reload re-resolves the same diagram id to a freshly-rendered figure.
    // Same id + new figure = update: the popup shows the new render's content.
    const first = diagramFigure("before");
    const { container, rerender } = render(DiagramZoomModal, { props: closed });
    await rerender({ diagramId: "d0", figure: first, onClose: () => {} });
    flushSync();
    await tick();
    expect(
      container.querySelector('[data-testid="diagram-zoom-content"] svg[data-marker="before"]'),
    ).not.toBeNull();

    const second = diagramFigure("after");
    await rerender({ diagramId: "d0", figure: second, onClose: () => {} });
    flushSync();
    await tick();
    expect(
      container.querySelector('[data-testid="diagram-zoom-content"] svg[data-marker="after"]'),
    ).not.toBeNull();
    expect(
      container.querySelector('[data-testid="diagram-zoom-content"] svg[data-marker="before"]'),
    ).toBeNull();
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
    expect(
      dialog.querySelector('[data-testid="diagram-zoom-content"] svg[data-marker="good"]'),
    ).not.toBeNull();
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
    expect(
      container.querySelector('[data-testid="diagram-zoom-content"] svg[data-marker="two"]'),
    ).not.toBeNull();
    expect(
      container.querySelector('[data-testid="diagram-zoom-content"] svg[data-marker="one"]'),
    ).toBeNull();
  });
});

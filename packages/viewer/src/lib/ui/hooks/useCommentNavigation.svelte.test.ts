import { describe, it, expect, vi, afterEach } from "vitest";
import { flushSync } from "svelte";
import { useCommentNavigation, type CommentNavigationDeps } from "./useCommentNavigation.svelte";

// Each mount opens an effect root (to host the hook's $effect) and attaches a
// window keydown listener; teardown closes the root, removing the listener, and
// clears any focused element left behind so tests don't leak focus into each other.
let teardown: (() => void) | null = null;

afterEach(() => {
  teardown?.();
  teardown = null;
  document.body.innerHTML = "";
  (document.activeElement as HTMLElement | null)?.blur?.();
});

function mount(deps: CommentNavigationDeps) {
  let nav!: { announcement: string };
  const cleanup = $effect.root(() => {
    nav = useCommentNavigation(deps);
  });
  flushSync(); // run the effect so the keydown listener is attached
  teardown = cleanup;
  return nav;
}

function press(key: string, modifiers: Partial<KeyboardEventInit> = {}) {
  window.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, ...modifiers }));
}

function deps(over: Partial<CommentNavigationDeps> = {}) {
  const navigable = vi.fn<() => string[]>(() => ["a", "b"]);
  const navigate = vi.fn<CommentNavigationDeps["navigate"]>(() => ({
    index: 0,
    total: 2,
    author: "You",
  }));
  const requestReplyFocus = vi.fn<CommentNavigationDeps["requestReplyFocus"]>(() => ({
    index: 0,
    total: 2,
    author: "You",
  }));
  return { navigable, navigate, requestReplyFocus, ...over };
}

describe("useCommentNavigation", () => {
  it("maps n to next and p to previous", () => {
    const d = deps();
    mount(d);

    press("n");
    expect(d.navigate).toHaveBeenLastCalledWith("next");

    press("p");
    expect(d.navigate).toHaveBeenLastCalledWith("prev");
  });

  it("announces the new position, including the author", () => {
    const d = deps();
    const nav = mount(d);

    press("n");
    expect(nav.announcement).toBe("Comment 1 of 2 by You");
  });

  it("omits the author when the name is blank", () => {
    const d = deps({ navigate: vi.fn(() => ({ index: 1, total: 3, author: "" })) });
    const nav = mount(d);

    press("n");
    expect(nav.announcement).toBe("Comment 2 of 3");
  });

  it("re-announces an identical position by changing the live-region text node", () => {
    // A polite live region only re-announces when its text node *changes*.
    // navigate() wraps around, so on a single-comment page (or onto a thread by
    // the same author at the same index) every press returns identical
    // {index, total, author} — a byte-identical string the screen reader would
    // ignore. The announcement must still change so it re-announces. See #542.
    const d = deps({ navigate: vi.fn(() => ({ index: 0, total: 1, author: "Mike" })) });
    const nav = mount(d);

    press("n");
    const first = nav.announcement;
    press("n");
    const second = nav.announcement;

    // The text node differs (so the SR speaks again)...
    expect(second).not.toBe(first);
    // ...but the spoken text (zero-width marker stripped) is unchanged.
    const spoken = (s: string) => s.replace(/\u200B/g, "");
    expect(spoken(first)).toBe("Comment 1 of 1 by Mike");
    expect(spoken(second)).toBe(spoken(first));

    // Each further press re-announces (text node changes) without the marker
    // ever stacking: the spoken text stays put and the raw string never grows
    // by more than the single zero-width marker.
    press("n");
    const third = nav.announcement;
    expect(third).not.toBe(second);
    expect(spoken(third)).toBe("Comment 1 of 1 by Mike");
    expect(third.length).toBeLessThanOrEqual(first.length + 1);
  });

  it("keeps the spoken text clean when stepping between distinct positions", () => {
    // When consecutive positions genuinely differ the visible string already
    // changes, so re-announcement never depended on the marker. Assert the
    // marker never corrupts the spoken text on that normal path.
    const navigate = vi
      .fn<CommentNavigationDeps["navigate"]>()
      .mockReturnValueOnce({ index: 0, total: 2, author: "Ann" })
      .mockReturnValueOnce({ index: 1, total: 2, author: "Bob" });
    const nav = mount(deps({ navigate }));
    const spoken = (s: string) => s.replace(/\u200B/g, "");

    press("n");
    expect(spoken(nav.announcement)).toBe("Comment 1 of 2 by Ann");
    press("n");
    expect(spoken(nav.announcement)).toBe("Comment 2 of 2 by Bob");
  });

  it("lets modifier combinations through to the browser", () => {
    const d = deps();
    mount(d);

    press("n", { metaKey: true });
    press("n", { ctrlKey: true });
    press("p", { altKey: true });

    expect(d.navigate).not.toHaveBeenCalled();
  });

  it("ignores keys other than n/p", () => {
    const d = deps();
    mount(d);

    press("j");
    press("ArrowDown");
    press("Enter");

    expect(d.navigate).not.toHaveBeenCalled();
  });

  it("does nothing when there are no navigable comments", () => {
    const d = deps({ navigable: vi.fn(() => []) });
    mount(d);

    press("n");

    expect(d.navigate).not.toHaveBeenCalled();
  });

  it.each(["input", "textarea", "select"] as const)(
    "does not navigate while focus is in a <%s>",
    (tag) => {
      const d = deps();
      mount(d);
      const el = document.createElement(tag);
      document.body.appendChild(el);
      el.focus();

      press("n");

      expect(d.navigate).not.toHaveBeenCalled();
    },
  );

  it("does not navigate when the event target is a contenteditable element", () => {
    const d = deps();
    mount(d);
    const el = document.createElement("div");
    // jsdom doesn't implement isContentEditable from the contentEditable
    // attribute, so define it directly; the guard reads this property.
    Object.defineProperty(el, "isContentEditable", { value: true, configurable: true });
    document.body.appendChild(el);

    // Dispatch from the element so e.target is the contenteditable node,
    // exercising the isEditable(e.target) branch regardless of focus handling.
    el.dispatchEvent(new KeyboardEvent("keydown", { key: "n", bubbles: true }));

    expect(d.navigate).not.toHaveBeenCalled();
  });

  it("r calls requestReplyFocus and announces the reply position", () => {
    const d = deps();
    const nav = mount(d);

    press("r");

    expect(d.requestReplyFocus).toHaveBeenCalledOnce();
    expect(nav.announcement).toBe("Replying to comment 1 of 2 by You");
    // r is purely a reply trigger; it must not also navigate.
    expect(d.navigate).not.toHaveBeenCalled();
  });

  it("consumes the event (preventDefault) on a successful r", () => {
    const d = deps();
    mount(d);

    const event = new KeyboardEvent("keydown", { key: "r", bubbles: true, cancelable: true });
    window.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(true);
  });

  it("r does nothing (no announcement, no navigation) when requestReplyFocus returns null", () => {
    const d = deps({ requestReplyFocus: vi.fn(() => null) });
    const nav = mount(d);

    press("r");

    expect(d.requestReplyFocus).toHaveBeenCalledOnce();
    expect(nav.announcement).toBe("");
    // A null result must not fall through to the n/p navigation path.
    expect(d.navigate).not.toHaveBeenCalled();
  });

  it("does not trigger reply-focus while focus is in a <textarea>", () => {
    const d = deps();
    mount(d);
    const el = document.createElement("textarea");
    document.body.appendChild(el);
    el.focus();

    press("r");

    expect(d.requestReplyFocus).not.toHaveBeenCalled();
  });

  it("lets modifier+r through to the browser", () => {
    const d = deps();
    mount(d);

    press("r", { metaKey: true });
    press("r", { ctrlKey: true });

    expect(d.requestReplyFocus).not.toHaveBeenCalled();
  });

  it("stops listening after teardown", () => {
    const d = deps();
    mount(d);
    teardown?.();
    teardown = null;

    press("n");

    expect(d.navigate).not.toHaveBeenCalled();
  });
});

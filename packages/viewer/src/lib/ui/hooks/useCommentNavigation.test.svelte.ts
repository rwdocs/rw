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
  return { navigable, navigate, ...over };
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

  it("stops listening after teardown", () => {
    const d = deps();
    mount(d);
    teardown?.();
    teardown = null;

    press("n");

    expect(d.navigate).not.toHaveBeenCalled();
  });
});

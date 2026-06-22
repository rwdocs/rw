<script lang="ts">
  import Button from "$lib/ui/primitives/Button.svelte";
  import { useElementSize } from "$lib/ui/hooks/useElementSize.svelte";

  interface Props {
    /** Submit the composed body. To preserve the draft on failure, the handler
     *  must call notify() and then rethrow — CommentForm swallows the rejection,
     *  keeps the text, and shows Retry. Resolving clears the textarea. */
    onSubmit: (body: string) => Promise<void>;
    onCancel?: () => void;
    placeholder?: string;
    autofocus?: boolean;
    /** When true, the action buttons (Cancel/Comment) are always shown.
     *  Use for forms the user explicitly summoned (e.g. the pending new
     *  comment), where hiding the buttons on blur would be surprising. */
    pinActions?: boolean;
    /** Called with the vertical distance from the form's outer border to the top of
     *  the textarea's content box (where the first line of text appears), whenever
     *  that distance changes. */
    onAnchor?: (offsetPx: number) => void;
    /** Extra classes appended to the <form> element — lets callers put their
     *  border/padding directly on the form so anchor measurements naturally
     *  include those offsets. */
    outerClass?: string;
    /** Draft text. Bindable so a caller can persist/restore the draft (e.g. the
     *  per-thread reply draft kept in the comments store). Omitted ⇒ the form
     *  manages its own text, unchanged from before. */
    value?: string;
  }

  let {
    onSubmit,
    onCancel,
    placeholder = "Write a comment...",
    autofocus = false,
    pinActions = false,
    onAnchor,
    outerClass = "",
    value = $bindable(""),
  }: Props = $props();

  let submitting = $state(false);
  let failed = $state(false);
  let focused = $state(false);
  let textareaRef: HTMLTextAreaElement | undefined = $state();
  let formRef: HTMLFormElement | undefined = $state();

  const formSize = useElementSize(() => formRef ?? null);
  const textareaSize = useElementSize(() => textareaRef ?? null);

  let showActions = $derived(pinActions || focused || value.trim().length > 0);

  // Auto-focus the textarea on mount. Deferred to rAF so the parent's
  // visibility-hidden-until-measured wrapper (CommentPanel's pinned margin-column
  // mode) has flipped to visible before we focus — focus() on a visibility:hidden
  // element is a spec no-op. (In the popover, pin=false, the wrapper is visible
  // from first paint, so the rAF is harmless there.)
  function autofocusTextarea(ta: HTMLTextAreaElement) {
    if (!autofocus) return;
    const id = requestAnimationFrame(() => ta.focus({ preventScroll: true }));
    return () => cancelAnimationFrame(id);
  }

  let lastReported: number | null = null;
  $effect(() => {
    if (!onAnchor || !formRef || !textareaRef) return;
    void formSize.version;
    void textareaSize.version;
    // Bounding rects rather than offsetTop because the form isn't positioned,
    // so its offsetParent isn't guaranteed to be formRef.
    const formRect = formRef.getBoundingClientRect();
    const taRect = textareaRef.getBoundingClientRect();
    const paddingTop = parseFloat(getComputedStyle(textareaRef).paddingTop) || 0;
    const offset = taRect.top - formRect.top + paddingTop;
    if (lastReported === null || Math.abs(offset - lastReported) > 0.5) {
      lastReported = offset;
      onAnchor(offset);
    }
  });

  function handleFocusOut(event: FocusEvent) {
    const next = event.relatedTarget as Node | null;
    const form = event.currentTarget as HTMLElement;
    if (!next || !form.contains(next)) {
      focused = false;
    }
  }

  // Grow the textarea to fit its content. Reads `value` so it re-runs on every
  // change — including programmatic resets after submit, which don't fire
  // `oninput` and would otherwise leave the textarea stuck at its previous
  // grown height.
  function autoGrowTextarea(ta: HTMLTextAreaElement) {
    void value;
    ta.style.height = "auto";
    ta.style.height = `${ta.scrollHeight}px`;
  }

  async function submit() {
    if (!value.trim() || submitting) return;
    submitting = true;
    failed = false;
    try {
      await onSubmit(value.trim());
      value = "";
    } catch {
      // onSubmit callers rethrow only after calling notify(); swallowing the
      // rejection here keeps this end-of-chain form free of unhandled rejections
      // and flips to Retry without clearing the draft.
      failed = true;
    } finally {
      submitting = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      submit();
    } else if (event.key === "Escape" && !event.isComposing) {
      event.preventDefault();
      // Leave the field so n/p comment navigation resumes; close the form too
      // when the caller wants Escape to dismiss it. Skipped mid-IME-composition
      // so Escape can cancel the composition instead of blurring the field.
      textareaRef?.blur();
      onCancel?.();
    }
  }

  function handleSubmit(event: SubmitEvent) {
    event.preventDefault();
    submit();
  }
</script>

<form
  bind:this={formRef}
  onsubmit={handleSubmit}
  onfocusin={() => (focused = true)}
  onfocusout={handleFocusOut}
  class="flex flex-col gap-2 {outerClass}"
>
  <textarea
    bind:this={textareaRef}
    bind:value
    onkeydown={handleKeydown}
    oninput={() => (failed = false)}
    {@attach autoGrowTextarea}
    {@attach autofocusTextarea}
    {placeholder}
    rows={1}
    class="
      w-full resize-none rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900
      placeholder-gray-400
      focus:border-blue-500 focus:ring-1 focus:ring-blue-500 focus:outline-none
      dark:border-neutral-600 dark:bg-neutral-800 dark:text-neutral-100 dark:placeholder-neutral-500
      dark:focus:border-blue-400 dark:focus:ring-blue-400
    "
  ></textarea>
  {#if showActions}
    <div class="flex justify-end gap-2">
      {#if onCancel}
        <Button variant="ghost" onclick={onCancel}>Cancel</Button>
      {/if}
      <Button
        type="submit"
        variant={failed ? "danger" : "primary"}
        disabled={!value.trim()}
        loading={submitting}
      >
        {failed ? "Retry" : "Comment"}
      </Button>
    </div>
  {/if}
</form>

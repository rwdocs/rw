<script lang="ts">
  import Button from "$lib/ui/primitives/Button.svelte";

  interface Props {
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
  }

  let {
    onSubmit,
    onCancel,
    placeholder = "Write a comment...",
    autofocus = false,
    pinActions = false,
    onAnchor,
    outerClass = "",
  }: Props = $props();

  let body = $state("");
  let submitting = $state(false);
  let focused = $state(false);
  let textareaRef: HTMLTextAreaElement | undefined = $state();
  let formRef: HTMLFormElement | undefined = $state();

  let showActions = $derived(pinActions || focused || body.trim().length > 0);

  // Defer to rAF so the parent's visibility-hidden-until-measured wrapper
  // (CommentSidebar) has flipped to visible before we focus — focus() on a
  // visibility:hidden element is a spec no-op.
  $effect(() => {
    if (!autofocus || !textareaRef) return;
    const ta = textareaRef;
    const id = requestAnimationFrame(() => ta.focus({ preventScroll: true }));
    return () => cancelAnimationFrame(id);
  });

  $effect(() => {
    if (!onAnchor || !formRef || !textareaRef) return;

    let lastReported: number | null = null;
    const measure = () => {
      if (!formRef || !textareaRef) return;
      // offsetTop is relative to offsetParent, which isn't guaranteed to be
      // formRef (form isn't positioned). Use bounding rects so the returned
      // offset is always relative to the form's outer border.
      const formRect = formRef.getBoundingClientRect();
      const taRect = textareaRef.getBoundingClientRect();
      const paddingTop = parseFloat(getComputedStyle(textareaRef).paddingTop) || 0;
      const offset = taRect.top - formRect.top + paddingTop;
      if (lastReported === null || Math.abs(offset - lastReported) > 0.5) {
        lastReported = offset;
        onAnchor?.(offset);
      }
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(formRef);
    return () => observer.disconnect();
  });

  function handleFocusOut(event: FocusEvent) {
    const next = event.relatedTarget as Node | null;
    const form = event.currentTarget as HTMLElement;
    if (!next || !form.contains(next)) {
      focused = false;
    }
  }

  // Re-run whenever body changes — including programmatic resets after submit,
  // which don't fire `oninput` and would otherwise leave the textarea stuck at
  // its previous grown height.
  $effect(() => {
    body;
    if (!textareaRef) return;
    textareaRef.style.height = "auto";
    textareaRef.style.height = `${textareaRef.scrollHeight}px`;
  });

  async function submit() {
    if (!body.trim() || submitting) return;
    submitting = true;
    try {
      await onSubmit(body.trim());
      body = "";
    } finally {
      submitting = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      submit();
    } else if (event.key === "Escape" && onCancel) {
      event.preventDefault();
      onCancel();
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
    bind:value={body}
    onkeydown={handleKeydown}
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
      <Button type="submit" variant="primary" disabled={!body.trim()} loading={submitting}>
        Comment
      </Button>
    </div>
  {/if}
</form>

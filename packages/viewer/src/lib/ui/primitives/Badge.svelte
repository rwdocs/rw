<script module lang="ts">
  type Intent = "neutral" | "info" | "warning" | "attention";
  type Size = "sm" | "md";

  // Tailwind's JIT needs full class strings present in source to compile them,
  // so intent/size classes are complete-string lookup tables rather than
  // interpolated `bg-${intent}-bg` that would fail to generate utilities.
  const INTENT_CLASSES: Record<Intent, string> = {
    // Neutral reads from the page's own surface scale rather than the status
    // triples so it blends in as a secondary/metadata chip (e.g. counters).
    neutral: "bg-bg-subtle text-fg-muted",
    info: "bg-info-bg text-info-fg",
    warning: "bg-warning-bg text-warning-fg",
    attention: "bg-attention-bg text-attention-fg",
  };

  const SIZE_CLASSES: Record<Size, string> = {
    sm: "px-1.5 py-0.5 text-xs",
    md: "px-2 py-0.5 text-sm",
  };
</script>

<script lang="ts">
  import type { HTMLAttributes } from "svelte/elements";
  import type { Snippet } from "svelte";

  // ARIA forbids naming a generic <span>, so screen readers would ignore
  // aria-label here and read only the visible text; `label` is the way in.
  interface Props extends Omit<HTMLAttributes<HTMLSpanElement>, "aria-label" | "aria-labelledby"> {
    intent?: Intent;
    size?: Size;
    /** Read by screen readers instead of the visible text, e.g. "3 comments" for "3". */
    label?: string;
    children?: Snippet;
  }

  let {
    intent = "neutral",
    size = "sm",
    label,
    class: extraClass = "",
    children,
    ...rest
  }: Props = $props();
</script>

<span
  {...rest}
  class="
    inline-flex items-center rounded-sm font-medium
    {INTENT_CLASSES[intent]}
    {SIZE_CLASSES[size]}
    {extraClass}
  "
>
  {#if label}
    <span aria-hidden="true">{@render children?.()}</span>
    <span class="sr-only">{label}</span>
  {:else if children}
    {@render children()}
  {/if}
</span>

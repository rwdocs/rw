<script module lang="ts">
  type Variant = "primary" | "secondary" | "ghost" | "danger";
  type Size = "xs" | "sm" | "md";
  type SizeKey = `${Size}-${"icon" | "text"}`;

  // Tailwind's JIT needs full class strings present in source to compile them,
  // so variants are expressed as complete-class lookup tables rather than
  // interpolation (`bg-${variant}-bg-solid` would fail to generate utilities).
  const VARIANT_CLASSES: Record<Variant, string> = {
    primary:
      "bg-accent-bg text-fg-on-solid hover:bg-accent-bg-hover focus-visible:outline-accent-fg",
    secondary:
      "bg-bg-raised text-fg-default border border-border-default hover:bg-bg-subtle focus-visible:outline-accent-fg",
    ghost: "bg-transparent text-fg-default hover:bg-bg-subtle focus-visible:outline-accent-fg",
    danger:
      "bg-danger-bg-solid text-fg-on-solid hover:bg-danger-bg-solid-hover focus-visible:outline-danger-fg",
  };

  // Radius lives in the size table (not the base class list) so xs can opt
  // for rounded-sm while sm/md keep rounded-md, without relying on Tailwind's
  // utility-sort order to break ties between two rounded utilities.
  const SIZE_CLASSES: Record<SizeKey, string> = {
    "xs-icon": "size-5 rounded-sm text-xs",
    "sm-icon": "size-7 rounded-md text-xs",
    "md-icon": "size-8 rounded-md text-sm",
    "xs-text": "rounded-sm px-1.5 py-0.5 text-xs",
    "sm-text": "rounded-md px-2 py-1 text-xs",
    "md-text": "rounded-md px-3 py-1.5 text-sm",
  };
</script>

<script lang="ts">
  import type { HTMLButtonAttributes } from "svelte/elements";
  import type { Snippet } from "svelte";

  interface Props extends HTMLButtonAttributes {
    variant?: Variant;
    size?: Size;
    iconOnly?: boolean;
    loading?: boolean;
    children?: Snippet;
  }

  let {
    variant = "primary",
    size = "md",
    iconOnly = false,
    loading = false,
    disabled = false,
    type = "button",
    class: extraClass = "",
    onclick,
    children,
    ...rest
  }: Props = $props();

  const inactive = $derived(disabled || loading);
  const sizeClass = $derived(SIZE_CLASSES[`${size}-${iconOnly ? "icon" : "text"}`]);
</script>

<!--
  Uses aria-disabled + an onclick guard instead of the native `disabled`
  attribute so disabled buttons remain focusable — screen-reader users
  can still tab to them and hear the disabled state announced. Styling
  keys off aria-disabled for the same reason.
-->
<button
  {...rest}
  {type}
  aria-disabled={inactive || undefined}
  aria-busy={loading || undefined}
  onclick={inactive ? undefined : onclick}
  class="
    inline-flex cursor-pointer items-center justify-center gap-1.5 font-medium transition-colors
    focus-visible:outline-2 focus-visible:outline-offset-2
    aria-disabled:cursor-not-allowed aria-disabled:opacity-50 aria-disabled:hover:bg-transparent
    {VARIANT_CLASSES[variant]}
    {sizeClass}
    {extraClass}
  "
>
  {#if children}
    {@render children()}
  {/if}
</button>

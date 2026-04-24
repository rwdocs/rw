<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  // Extends HTMLButtonAttributes so callers can pass any button attribute —
  // including the ARIA relationship attrs (`aria-controls`, `aria-haspopup`,
  // `aria-describedby`) that Popover's `controlProps` wires through when
  // IconButton is used as a Popover trigger.
  interface Props extends HTMLButtonAttributes {
    "aria-label": string;
    active?: boolean;
    children: Snippet;
  }

  let {
    "aria-label": ariaLabel,
    active = false,
    class: extraClass = "",
    children,
    ...rest
  }: Props = $props();
</script>

<button
  {...rest}
  class="
    flex size-8 cursor-pointer items-center justify-center rounded-sm border border-border-default
    bg-bg-raised text-fg-muted
    hover:bg-bg-subtle hover:text-fg-default
    focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent-fg
    active:bg-bg-subtle
    {extraClass}
  "
  class:border-border-strong={active}
  aria-label={ariaLabel}
>
  {@render children()}
</button>

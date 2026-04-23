<script lang="ts">
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLQuoteElement> {
    prefix?: string;
    exact: string;
    suffix?: string;
  }

  let { prefix, exact, suffix, class: extraClass = "", ...rest }: Props = $props();
</script>

<!--
  Whitespace between {#if} / <span> / <mark> is deliberately collapsed so
  "…prefix" + exact + "suffix…" concatenate without stray spaces when
  either context span is present.
-->
<blockquote
  {...rest}
  class="
    border-l-2 border-(--highlight-comment-border) pl-3 text-sm text-fg-muted italic
    {extraClass}
  "
>
  {#if prefix}<span class="opacity-70">…{prefix}</span>{/if}<mark
    class="rounded-sm bg-(--highlight-comment) px-0.5 text-inherit not-italic">{exact}</mark
  >{#if suffix}<span class="opacity-70">{suffix}…</span>{/if}
</blockquote>

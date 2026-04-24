<script module lang="ts">
  type Variant = "person" | "ai" | "initials";

  // Per-variant typography. Initials carry information so they need stronger
  // contrast (text-fg-default + font-medium); icon variants are decorative
  // and read as a muted glyph on the shared bg-bg-subtle surface.
  const VARIANT_CLASSES: Record<Variant, string> = {
    person: "text-fg-muted",
    ai: "text-fg-muted",
    initials: "text-xs font-medium text-fg-default",
  };

  // Heroicons solid: "user" (person) and "sparkles" (ai).
  const ICON_PATHS: Record<"person" | "ai", string> = {
    person:
      "M7.5 6a4.5 4.5 0 1 1 9 0 4.5 4.5 0 0 1-9 0ZM3.751 20.105a8.25 8.25 0 0 1 16.498 0 .75.75 0 0 1-.437.695A18.683 18.683 0 0 1 12 22.5c-2.786 0-5.433-.608-7.812-1.7a.75.75 0 0 1-.437-.695Z",
    ai: "M9 4.5a.75.75 0 0 1 .721.544l.813 2.846a3.75 3.75 0 0 0 2.576 2.576l2.846.813a.75.75 0 0 1 0 1.442l-2.846.813a3.75 3.75 0 0 0-2.576 2.576l-.813 2.846a.75.75 0 0 1-1.442 0l-.813-2.846a3.75 3.75 0 0 0-2.576-2.576l-2.846-.813a.75.75 0 0 1 0-1.442l2.846-.813a3.75 3.75 0 0 0 2.576-2.576L8.279 5.044A.75.75 0 0 1 9 4.5ZM18 1.5a.75.75 0 0 1 .728.568l.258 1.036c.236.94.97 1.674 1.91 1.91l1.036.258a.75.75 0 0 1 0 1.456l-1.036.258c-.94.236-1.674.97-1.91 1.91l-.258 1.036a.75.75 0 0 1-1.456 0l-.258-1.036a2.625 2.625 0 0 0-1.91-1.91l-1.036-.258a.75.75 0 0 1 0-1.456l1.036-.258a2.625 2.625 0 0 0 1.91-1.91l.258-1.036A.75.75 0 0 1 18 1.5ZM16.5 15a.75.75 0 0 1 .712.513l.394 1.183c.15.447.5.799.948.948l1.183.395a.75.75 0 0 1 0 1.422l-1.183.395c-.447.15-.799.5-.948.948l-.395 1.183a.75.75 0 0 1-1.422 0l-.395-1.183a1.5 1.5 0 0 0-.948-.948l-1.183-.395a.75.75 0 0 1 0-1.422l1.183-.395c.447-.15.799-.5.948-.948l.395-1.183A.75.75 0 0 1 16.5 15Z",
  };

  function computeInitials(name: string): string {
    const tokens = name.trim().split(/\s+/).filter(Boolean);
    if (tokens.length === 0) return "?";
    if (tokens.length === 1) return tokens[0][0]?.toUpperCase() ?? "?";
    return (tokens[0][0] + tokens[1][0]).toUpperCase();
  }
</script>

<script lang="ts">
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLSpanElement> {
    variant: Variant;
    size?: number;
    /** Source text for initials (variant="initials"). */
    name?: string;
    /** Image URL — overrides variant when set. */
    src?: string;
  }

  let { variant, size = 24, name = "", src, class: extraClass = "", ...rest }: Props = $props();

  const iconSize = $derived(Math.round(size * 0.7));
</script>

{#if src}
  <!-- alt="" because callers render the author's name as visible text alongside the avatar. -->
  <img
    {...rest}
    {src}
    alt=""
    width={size}
    height={size}
    class="rounded-full object-cover {extraClass}"
    style:width="{size}px"
    style:height="{size}px"
  />
{:else}
  <span
    {...rest}
    class="
      inline-flex items-center justify-center rounded-full bg-bg-subtle
      {VARIANT_CLASSES[variant]}
      {extraClass}
    "
    style:width="{size}px"
    style:height="{size}px"
    aria-hidden="true"
  >
    {#if variant === "initials"}
      {computeInitials(name)}
    {:else}
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="currentColor"
        style:width="{iconSize}px"
        style:height="{iconSize}px"
      >
        <path fill-rule="evenodd" d={ICON_PATHS[variant]} clip-rule="evenodd" />
      </svg>
    {/if}
  </span>
{/if}

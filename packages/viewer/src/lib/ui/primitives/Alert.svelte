<script module lang="ts">
  type Intent = "info" | "success" | "warning" | "danger" | "attention";

  // Tailwind's JIT needs full class strings present in source to compile them,
  // so intent classes are a complete-string lookup table rather than an
  // interpolated `bg-${intent}-bg` that would fail to generate utilities.
  const INTENT_CLASSES: Record<Intent, string> = {
    info: "text-info-fg bg-info-bg border-info-border",
    success: "text-success-fg bg-success-bg border-success-border",
    warning: "text-warning-fg bg-warning-bg border-warning-border",
    danger: "text-danger-fg bg-danger-bg border-danger-border",
    attention: "text-attention-fg bg-attention-bg border-attention-border",
  };

  // Urgent intents get role="alert" (assertive live region) so assistive tech
  // interrupts the user. Non-urgent intents use role="status" (polite) so a
  // success toast doesn't steal focus mid-sentence.
  const URGENT: ReadonlySet<Intent> = new Set(["danger", "attention"]);
</script>

<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    intent: Intent;
    title?: string;
    dismissible?: boolean;
    onDismiss?: () => void;
    children?: Snippet;
    class?: string;
  }

  let {
    intent,
    title,
    dismissible = false,
    onDismiss,
    class: extraClass = "",
    children,
  }: Props = $props();

  const role = $derived(URGENT.has(intent) ? "alert" : "status");
</script>

<div
  {role}
  class="
    flex items-start gap-3 rounded-md border px-3 py-2 text-sm
    {INTENT_CLASSES[intent]}
    {extraClass}
  "
>
  <div class="min-w-0 flex-1">
    {#if title}
      <div class="font-semibold">{title}</div>
    {/if}
    {#if children}
      {@render children()}
    {/if}
  </div>
  {#if dismissible}
    <!--
      Plain <button> rather than the Button primitive: the dismiss control
      needs to inherit the Alert's intent color (text-current) and a ghost
      Button hardcodes text-fg-default, which Tailwind 4 sorts after
      text-current so it would always win specificity-wise.
    -->
    <button
      type="button"
      onclick={onDismiss}
      aria-label="Dismiss"
      class="
        -m-1 inline-flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-sm
        text-current opacity-70 transition-opacity
        hover:opacity-100
        focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-current
      "
    >
      <svg
        aria-hidden="true"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        class="size-4"
      >
        <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </button>
  {/if}
</div>

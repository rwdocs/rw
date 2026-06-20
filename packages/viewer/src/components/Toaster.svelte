<script module lang="ts">
  import type { NotifyIntent } from "../types/notify";

  // Map the framework-neutral notify intents onto Alert's vocabulary.
  // Alert derives role="alert" (assertive) for `danger`, role="status" otherwise.
  const ALERT_INTENT: Record<NotifyIntent, "info" | "success" | "warning" | "danger"> = {
    info: "info",
    success: "success",
    warning: "warning",
    error: "danger",
  };
</script>

<script lang="ts">
  import { getRwContext } from "$lib/context";
  import Alert from "$lib/ui/primitives/Alert.svelte";

  const { ui } = getRwContext();
</script>

{#if ui.toasts.length > 0}
  <!-- Fixed, viewport-anchored: bottom-center on mobile, bottom-right ≥640px.
       The stack hugs its content (no full-bleed width), so it doesn't need
       `pointer-events-none`. A `pointer-events-none` ancestor over an interactive
       child makes Chrome flicker the cursor on mouse-move, so it is deliberately
       absent here. -->
  <div
    class="
      fixed bottom-4 left-1/2 z-toast flex -translate-x-1/2 flex-col items-center gap-2
      sm:right-4 sm:left-auto sm:translate-x-0 sm:items-end
    "
  >
    {#each ui.toasts as toast (toast.id)}
      <Alert
        intent={ALERT_INTENT[toast.intent]}
        dismissible
        onDismiss={() => ui.dismissToast(toast.id)}
        class="w-80 max-w-[90vw] shadow-lg"
      >
        {toast.message}
      </Alert>
    {/each}
  </div>
{/if}

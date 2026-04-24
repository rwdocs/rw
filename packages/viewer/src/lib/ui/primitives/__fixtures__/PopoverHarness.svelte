<!--
  Harness for Popover tests. Exposes the bound `open` state on a root
  data-attribute so tests can assert dismissal without introspecting the
  Popover's internals, and conditionally renders the `trigger` snippet so a
  single harness covers all three modes (free / external anchor / inline
  trigger).
-->
<script lang="ts">
  import type { ComponentProps } from "svelte";
  import Popover from "../Popover.svelte";

  type Base = Omit<ComponentProps<typeof Popover>, "children" | "trigger" | "open">;

  interface Props extends Base {
    body?: string;
    triggerLabel?: string;
    initialOpen?: boolean;
  }

  let { body = "content", triggerLabel, initialOpen = false, ...rest }: Props = $props();

  // svelte-ignore state_referenced_locally — capturing only the initial
  // value is intentional here; tests mutate `open` via Popover's bindable,
  // so a reactive tie to the prop would fight the bind round-trip.
  let open = $state(initialOpen);
</script>

<div data-testid="pp-harness" data-open={String(open)}>
  {#if triggerLabel}
    <Popover bind:open {...rest}>
      {#snippet trigger({ controlProps })}
        <button
          type="button"
          data-testid="pp-trigger"
          onclick={() => (open = !open)}
          {...controlProps}
        >
          {triggerLabel}
        </button>
      {/snippet}
      <span data-testid="pp-body">{body}</span>
    </Popover>
  {:else}
    <Popover bind:open {...rest}>
      <span data-testid="pp-body">{body}</span>
    </Popover>
  {/if}
</div>

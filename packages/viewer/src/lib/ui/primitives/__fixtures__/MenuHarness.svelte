<!--
  Harness for Menu tests. The anchor is passed in externally (matching the
  Popover harness pattern) so tests can mock getBoundingClientRect on it
  before mounting. Exposes the bound `open` state via data-open on the
  wrapper so tests can assert dismissal without introspecting Menu internals.
-->
<script lang="ts">
  import { Menu } from "../Menu";

  interface Item {
    label: string;
    href?: string;
    disabled?: boolean;
  }

  interface Props {
    anchorEl: HTMLElement;
    items?: Item[];
    initialOpen?: boolean;
    onItemClick?: (label: string) => void;
    ariaLabel?: string;
  }

  let {
    anchorEl,
    items = [{ label: "First" }, { label: "Second" }, { label: "Third" }],
    initialOpen = false,
    onItemClick,
    ariaLabel = "Test menu",
  }: Props = $props();

  // svelte-ignore state_referenced_locally — capturing only the initial
  // value is intentional; tests drive `open` via bind:open into the Menu.
  let open = $state(initialOpen);
</script>

<div data-testid="m-harness" data-open={String(open)}>
  <Menu.Root bind:open {anchorEl} aria-label={ariaLabel}>
    {#each items as item (item.label)}
      <Menu.Item
        href={item.href}
        disabled={item.disabled}
        onclick={() => onItemClick?.(item.label)}
      >
        {item.label}
      </Menu.Item>
    {/each}
  </Menu.Root>
</div>

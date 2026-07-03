<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    /** Accessible name for the set of related controls (e.g. "Zoom controls"). */
    "aria-label"?: string;
    children: Snippet;
    class?: string;
  }

  let { "aria-label": ariaLabel, children, class: extraClass = "" }: Props = $props();
</script>

<!--
  A row of buttons welded into one segmented control. Compose it with IconButton
  or Button children:

    <ButtonGroup aria-label="Zoom controls">
      <IconButton aria-label="Zoom out" .../>
      <Button variant="secondary" .../>
      <IconButton aria-label="Zoom in" .../>
    </ButtonGroup>

  Each child keeps its own border, so a hover/active background stays inside that
  border instead of painting over it; adjacent borders overlap into a single
  hairline divider, and only the outer corners are rounded.
-->
<div role="group" aria-label={ariaLabel} class="rw-button-group {extraClass}">
  {@render children()}
</div>

<style>
  .rw-button-group {
    display: inline-flex;
    align-items: center;
  }
  /* Square off every segment, then round only the two outer ends. 4px matches
     the design system's small radius (IconButton's rounded-sm). */
  .rw-button-group > :global(button) {
    border-radius: 0;
  }
  .rw-button-group > :global(button:first-child) {
    border-top-left-radius: 4px;
    border-bottom-left-radius: 4px;
  }
  .rw-button-group > :global(button:last-child) {
    border-top-right-radius: 4px;
    border-bottom-right-radius: 4px;
  }
  /* Overlap adjacent borders so the seam reads as one hairline, not two. */
  .rw-button-group > :global(button + button) {
    margin-left: -1px;
  }
  /* Lift a hovered/focused segment so its border and focus ring sit above the
     neighbour's overlapping border. */
  .rw-button-group > :global(button:hover),
  .rw-button-group > :global(button:focus-visible) {
    position: relative;
    z-index: 1;
  }
</style>

<!--
  Minimal harness that exercises useAnchorOffset through a real component
  lifecycle (mount / prop-change / unmount), so its internal $effect actually
  runs. Tests read the reactive values from data-* attributes rather than from
  a returned handle, since runes hooks can only be called from a component.

  `el` is a slight misnomer when a Range is passed, but `target` and `anchor`
  collide with @testing-library/svelte's mount options.
-->
<script lang="ts">
  import { useAnchorOffset } from "../useAnchorOffset.svelte";

  interface Props {
    el: Element | Range | null;
  }

  let { el }: Props = $props();

  const offset = useAnchorOffset(() => el);
</script>

<div
  data-testid="anchor-offset"
  data-top={offset.top}
  data-left={offset.left}
  data-width={offset.width}
  data-height={offset.height}
  data-measured={String(offset.measured)}
></div>

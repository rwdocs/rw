<!--
  Minimal harness that exercises useRangeRect through a real component
  lifecycle (mount / prop-change / unmount), so its internal $effect actually
  runs. Tests read the reactive values from data-* attributes rather than from
  a returned handle, since runes hooks can only be called from a component.
-->
<script lang="ts">
  import { useRangeRect } from "../useRangeRect.svelte";

  interface Props {
    range: Range | null;
  }

  let { range }: Props = $props();

  const rect = useRangeRect(() => range);
</script>

<div
  data-testid="range-rect"
  data-top={rect.top}
  data-left={rect.left}
  data-width={rect.width}
  data-height={rect.height}
  data-measured={String(rect.measured)}
></div>

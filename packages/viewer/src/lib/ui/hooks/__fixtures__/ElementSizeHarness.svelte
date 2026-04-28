<!--
  Minimal harness that exercises useElementSize through a real component
  lifecycle (mount / prop-change / unmount), so its internal $effect actually
  runs. Tests read the reactive values from data-* attributes rather than from
  a returned handle, since runes hooks can only be called from a component.
-->
<script lang="ts">
  import { useElementSize } from "../useElementSize.svelte";

  interface Props {
    el: HTMLElement | null;
  }

  let { el }: Props = $props();

  const size = useElementSize(() => el);
</script>

<div
  data-testid="element-size"
  data-width={size.width}
  data-height={size.height}
  data-version={size.version}
></div>

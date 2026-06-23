<!--
  Minimal harness that exercises useElementMove through a real component
  lifecycle (mount / prop-change / unmount), so its internal $effect actually
  runs. Tests read the reactive version from a data-* attribute.
-->
<script lang="ts">
  import { useElementMove } from "../useElementMove.svelte";

  interface Props {
    el: HTMLElement | null;
  }

  let { el }: Props = $props();

  const move = useElementMove(() => el);
</script>

<div data-testid="element-move" data-version={move.version}></div>

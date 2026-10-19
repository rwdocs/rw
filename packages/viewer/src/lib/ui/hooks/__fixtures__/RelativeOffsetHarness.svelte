<!--
  Minimal harness that exercises useRelativeOffset through a real component
  lifecycle, so its internal $effect actually runs.
-->
<script lang="ts">
  import { useRelativeOffset } from "../useRelativeOffset.svelte";

  interface Props {
    outer: HTMLElement;
    inner: HTMLElement;
    inset: (inner: HTMLElement, innerRect: DOMRect) => number;
    onChange?: (offsetPx: number) => void;
  }

  let { outer, inner, inset, onChange }: Props = $props();

  useRelativeOffset(
    () => outer,
    () => inner,
    (el, rect) => inset(el, rect),
    () => onChange,
  );
</script>

<script lang="ts">
  import type { SVGAttributes } from "svelte/elements";

  interface Props extends SVGAttributes<SVGSVGElement> {
    class?: string;
  }

  let { class: extraClass = "size-4", ...rest }: Props = $props();
</script>

<svg
  {...rest}
  class={extraClass}
  fill="none"
  viewBox="0 0 24 24"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d="M6 18L18 6M6 6l12 12" />
</svg>

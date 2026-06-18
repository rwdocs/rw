<script module lang="ts">
  type Direction = "right" | "down" | "left";
  type Size = "sm" | "md";

  // Tailwind's JIT needs full class strings present in source to compile them,
  // so the rotation/size classes are complete-string lookup tables rather than
  // interpolated values that would fail to generate utilities.
  const ROTATE_CLASSES: Record<Direction, string> = {
    right: "rotate-0",
    down: "rotate-90",
    left: "rotate-180",
  };

  const SIZE_CLASSES: Record<Size, string> = {
    sm: "size-3.5",
    md: "size-4",
  };
</script>

<script lang="ts">
  import type { SVGAttributes } from "svelte/elements";

  interface Props extends SVGAttributes<SVGSVGElement> {
    /** Which way the chevron points. Defaults to `right`. */
    direction?: Direction;
    /** Icon size — `sm` (14px) pairs with `text-xs`, `md` (16px) with `text-sm`. */
    size?: Size;
    class?: string;
  }

  let { direction = "right", size = "sm", class: extraClass = "", ...rest }: Props = $props();
</script>

<svg
  {...rest}
  class="{SIZE_CLASSES[size]} {ROTATE_CLASSES[direction]} {extraClass}"
  fill="currentColor"
  viewBox="0 0 20 20"
>
  <path
    fill-rule="evenodd"
    clip-rule="evenodd"
    d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z"
  />
</svg>

<script lang="ts">
  interface Props {
    x: number;
    y: number;
    onAdd: () => void;
  }

  let { x, y, onAdd }: Props = $props();
</script>

<div
  class="
    fixed z-50 flex items-center gap-1.5 rounded-lg border border-gray-200 bg-white px-3 py-1.5
    shadow-lg
    dark:border-neutral-600 dark:bg-neutral-700
  "
  style="left: {x}px; top: {y}px; transform: translate(-50%, -100%) translateY(-8px);"
>
  <button
    type="button"
    onclick={onAdd}
    class="
      flex items-center gap-1.5 text-sm font-medium text-gray-700 transition-colors
      hover:text-blue-600
      dark:text-neutral-200
      dark:hover:text-blue-400
    "
  >
    <svg class="size-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        d="M7 8h10M7 12h4m1 8l-4-4H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-3l-4 4z"
      />
    </svg>
    Add comment
  </button>
</div>

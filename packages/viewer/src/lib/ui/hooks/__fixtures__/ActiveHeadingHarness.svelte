<script lang="ts">
  import { useActiveHeading, type ActiveHeading } from "../useActiveHeading.svelte";

  interface Props {
    headingIds: string[];
    onInit?: (handle: ActiveHeading) => void;
  }

  let { headingIds, onInit }: Props = $props();

  const handle = useActiveHeading(() => headingIds);
  // svelte-ignore state_referenced_locally
  onInit?.(handle);
</script>

<div data-testid="active-heading" data-active={handle.activeId ?? ""}></div>

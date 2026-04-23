<!--
  Minimal harness so tests can render <Badge> with plain text children
  without plumbing createRawSnippet at every call site.
-->
<script lang="ts">
  import type { ComponentProps } from "svelte";
  import Badge from "../Badge.svelte";

  type HarnessProps = Omit<ComponentProps<typeof Badge>, "children"> & { body?: string };

  let { body = "Badge", ...rest }: HarnessProps = $props();
</script>

<Badge {...rest}>{body}</Badge>

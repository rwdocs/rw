<!--
  Minimal harness so tests can render <Button> with plain text children
  without plumbing createRawSnippet at every call site.
-->
<script lang="ts">
  import type { ComponentProps } from "svelte";
  import Button from "../Button.svelte";

  type HarnessProps = Omit<ComponentProps<typeof Button>, "children"> & { label?: string };

  let { label = "Go", ...rest }: HarnessProps = $props();
</script>

<Button {...rest}>{label}</Button>

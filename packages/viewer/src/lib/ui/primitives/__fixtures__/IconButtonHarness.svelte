<!--
  Minimal harness so tests can render <IconButton> with an inline child
  without plumbing createRawSnippet at every call site.
-->
<script lang="ts">
  import type { ComponentProps } from "svelte";
  import IconButton from "../IconButton.svelte";

  type HarnessProps = Omit<ComponentProps<typeof IconButton>, "children">;

  let props: HarnessProps = $props();
</script>

<IconButton {...props}>
  <svg data-testid="icon" viewBox="0 0 24 24"><path d="M0 0h24v24H0z" /></svg>
</IconButton>

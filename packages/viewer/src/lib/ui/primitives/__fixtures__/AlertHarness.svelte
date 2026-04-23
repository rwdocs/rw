<!--
  Minimal harness so tests can render <Alert> with plain text children
  without plumbing createRawSnippet at every call site.
-->
<script lang="ts">
  import type { ComponentProps } from "svelte";
  import Alert from "../Alert.svelte";

  type HarnessProps = Omit<ComponentProps<typeof Alert>, "children"> & { body?: string };

  let { body = "Something happened", ...rest }: HarnessProps = $props();
</script>

<Alert {...rest}>{body}</Alert>

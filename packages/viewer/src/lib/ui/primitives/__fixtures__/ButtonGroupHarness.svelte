<!--
  Minimal harness so tests can render <ButtonGroup> with inline button children
  without plumbing createRawSnippet at every call site.
-->
<script lang="ts">
  import ButtonGroup from "../ButtonGroup.svelte";
  import IconButton from "../IconButton.svelte";
  import Button from "../Button.svelte";

  let { label }: { label: string } = $props();
</script>

<ButtonGroup aria-label={label}>
  <IconButton aria-label="Zoom out">
    <svg viewBox="0 0 24 24"><path d="M5 12h14" /></svg>
  </IconButton>
  <Button variant="secondary" size="sm" aria-label="Reset zoom">100%</Button>
  <IconButton aria-label="Zoom in">
    <svg viewBox="0 0 24 24"><path d="M12 5v14M5 12h14" /></svg>
  </IconButton>
</ButtonGroup>

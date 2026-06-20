<!--
  Sets a minimal RwContext (Toaster only reads `ui`) so the component can be
  rendered in isolation. Tests pass a pre-seeded Ui instance.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { setRwContext, type RwContext } from "$lib/context";
  import { Ui } from "../../state/ui.svelte";
  import Toaster from "../Toaster.svelte";

  interface Props {
    ui?: Ui;
  }

  let { ui = new Ui() }: Props = $props();

  untrack(() => setRwContext({ ui } as unknown as RwContext));
</script>

<Toaster />

<script lang="ts">
  import { setRwContext, type RwContext } from "../../lib/context";
  import Kit from "../Kit.svelte";

  interface Props {
    clear?: () => void;
  }

  let { clear = () => {} }: Props = $props();

  setRwContext({ page: { clear: () => clear() } } as unknown as RwContext);
</script>

<Kit />

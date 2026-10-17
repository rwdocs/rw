<!--
  Sets a real RwContext around PageComments with inert API clients, so tests
  drive it by seeding the comments store rather than through network calls.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { setRwContext, type RwContext } from "$lib/context";
  import type { ApiClient } from "../../api/client";
  import { Page } from "../../state/page.svelte";
  import type { Comments } from "../../state/comments.svelte";
  import PageComments from "../comments/PageComments.svelte";

  let { comments }: { comments: Comments } = $props();

  untrack(() =>
    setRwContext({
      page: new Page({} as ApiClient, { embedded: true }),
      comments,
      notify: () => {},
    } as unknown as RwContext),
  );
</script>

<PageComments />

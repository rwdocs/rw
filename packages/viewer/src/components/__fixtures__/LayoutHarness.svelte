<!--
  Sets a real RwContext around Layout with inert API clients, so tests drive
  it by seeding state rather than through network calls.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { setRwContext, type RwContext } from "$lib/context";
  import type { ApiClient } from "../../api/client";
  import type { CommentApiClient } from "../../api/comments";
  import { Router } from "../../state/router.svelte";
  import { Page } from "../../state/page.svelte";
  import type { Navigation } from "../../state/navigation.svelte";
  import { Ui } from "../../state/ui.svelte";
  import { Comments } from "../../state/comments.svelte";
  import Layout from "../Layout.svelte";

  interface Props {
    navigation: Navigation;
    ui?: Ui;
  }

  let { navigation, ui = new Ui() }: Props = $props();

  untrack(() =>
    setRwContext({
      router: new Router({ embedded: true }),
      page: new Page({} as ApiClient),
      navigation,
      ui,
      comments: new Comments({} as CommentApiClient, () => {}),
      notify: () => {},
    } as unknown as RwContext),
  );
</script>

<Layout>
  <p>content</p>
</Layout>

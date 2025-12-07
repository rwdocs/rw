<script lang="ts">
  import { onMount } from "svelte";
  import { navigation } from "../stores/navigation";
  import NavigationSidebar from "./NavigationSidebar.svelte";
  import TocSidebar from "./TocSidebar.svelte";
  import Breadcrumbs from "./Breadcrumbs.svelte";
  import { page } from "../stores/page";
  import type { Snippet } from "svelte";

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();

  onMount(() => {
    navigation.load();
  });
</script>

<div class="h-full flex">
  <!-- Navigation Sidebar -->
  <aside
    class="w-[280px] flex-shrink-0 border-r border-gray-200 overflow-y-auto hidden md:block"
  >
    <div class="p-4">
      <a href="/" class="block mb-6">
        <span class="text-xl font-semibold text-gray-900">Docstage</span>
      </a>
      <NavigationSidebar />
    </div>
  </aside>

  <!-- Main Content -->
  <main class="flex-1 overflow-y-auto">
    <div class="max-w-4xl mx-auto px-8 py-6">
      {#if $page.data}
        <Breadcrumbs breadcrumbs={$page.data.breadcrumbs} />
      {/if}
      {@render children()}
    </div>
  </main>

  <!-- Table of Contents Sidebar -->
  {#if $page.data && $page.data.toc.length > 0}
    <aside
      class="w-[240px] flex-shrink-0 border-l border-gray-200 overflow-y-auto hidden lg:block"
    >
      <div class="p-4 sticky top-0">
        <TocSidebar toc={$page.data.toc} />
      </div>
    </aside>
  {/if}
</div>

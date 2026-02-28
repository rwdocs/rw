<script lang="ts">
  import { getRwContext } from "../lib/context";

  const { router } = getRwContext();
</script>

<div class="flex h-full items-center justify-center">
  <div class="text-center">
    <h1 class="mb-4 text-6xl font-bold text-gray-300">404</h1>
    <p class="mb-8 text-xl text-gray-600">Page not found</p>
    <a href={router.prefixPath("/")} class="text-blue-600 hover:text-blue-800 hover:underline">
      Go to home
    </a>
  </div>
</div>

<script lang="ts">
  import { LOADING_SHOW_DELAY } from "../lib/constants";

  const COMPLETION_DURATION = 300;

  interface Props {
    loading: boolean;
  }

  let { loading }: Props = $props();

  type AnimationState = "idle" | "running" | "completing";
  let animationState = $state<AnimationState>("idle");

  $effect(() => {
    if (loading) {
      // Only show progress bar if loading takes longer than threshold
      const timeout = setTimeout(() => {
        animationState = "running";
      }, LOADING_SHOW_DELAY);
      return () => clearTimeout(timeout);
    }

    // Not loading - if bar was visible, animate completion then hide
    if (animationState !== "idle") {
      animationState = "completing";
      const timeout = setTimeout(() => {
        animationState = "idle";
      }, COMPLETION_DURATION);
      return () => clearTimeout(timeout);
    }
  });
</script>

{#if animationState !== "idle"}
  <div
    class="absolute inset-x-0 top-0 z-50 h-0.5 overflow-hidden"
    role="progressbar"
    aria-label="Page loading"
    aria-busy={animationState === "running"}
  >
    <div
      class="
        h-full origin-left bg-blue-500 will-change-[transform,opacity] dark:bg-blue-400
        {animationState === 'running' ? 'animate-trickle' : 'animate-complete'}"
    ></div>
  </div>
{/if}

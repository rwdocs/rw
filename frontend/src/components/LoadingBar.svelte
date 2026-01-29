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
    class="fixed top-0 left-0 right-0 z-50 h-0.5 overflow-hidden"
    role="progressbar"
    aria-label="Page loading"
    aria-busy={animationState === "running"}
  >
    <div
      class="h-full bg-blue-500 origin-left {animationState === 'running'
        ? 'animate-trickle'
        : 'animate-complete'}"
    ></div>
  </div>
{/if}

<style>
  @keyframes trickle {
    0% {
      transform: scaleX(0);
    }
    20% {
      transform: scaleX(0.3);
    }
    50% {
      transform: scaleX(0.6);
    }
    80% {
      transform: scaleX(0.8);
    }
    100% {
      transform: scaleX(0.9);
    }
  }

  @keyframes complete {
    0% {
      transform: scaleX(0.9);
      opacity: 1;
    }
    50% {
      transform: scaleX(1);
      opacity: 1;
    }
    100% {
      transform: scaleX(1);
      opacity: 0;
    }
  }

  .animate-trickle,
  .animate-complete {
    will-change: transform, opacity;
  }

  .animate-trickle {
    animation: trickle 10s ease-out forwards;
  }

  .animate-complete {
    animation: complete 0.3s ease-out forwards;
  }
</style>

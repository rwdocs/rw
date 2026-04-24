<script lang="ts">
  const COMPLETION_DURATION = 300;

  interface Props {
    loading: boolean;
    threshold?: number;
  }

  let { loading, threshold = 300 }: Props = $props();

  type AnimationState = "idle" | "running" | "completing";
  let animationState = $state<AnimationState>("idle");

  $effect(() => {
    if (loading) {
      const timeout = setTimeout(() => {
        animationState = "running";
      }, threshold);
      return () => clearTimeout(timeout);
    }

    if (animationState === "running") {
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
    class="absolute inset-x-0 top-0 z-overlay h-0.5 overflow-hidden"
    role="progressbar"
    aria-label="Page loading"
    aria-busy={animationState === "running"}
  >
    <div
      class="
        h-full origin-left bg-accent-bg will-change-[transform,opacity]
        {animationState === 'running' ? 'animate-trickle' : 'animate-complete'}"
    ></div>
  </div>
{/if}

<script lang="ts">
  import { onMount } from "svelte";
  import { getRwContext } from "$lib/context";
  import Button from "$lib/ui/primitives/Button.svelte";
  import IconButton from "$lib/ui/primitives/IconButton.svelte";
  import Badge from "$lib/ui/primitives/Badge.svelte";
  import Avatar from "$lib/ui/primitives/Avatar.svelte";
  import Alert from "$lib/ui/primitives/Alert.svelte";
  import Popover from "$lib/ui/primitives/Popover.svelte";
  import { Menu } from "$lib/ui/primitives/Menu";
  import LoadingBar from "$lib/ui/primitives/LoadingBar.svelte";
  import LoadingSkeleton from "$lib/ui/primitives/LoadingSkeleton.svelte";
  import Quote from "$lib/ui/primitives/Quote.svelte";

  const { page } = getRwContext();

  let popoverOpen = $state(false);
  let menuAnchor: HTMLButtonElement | undefined = $state();
  let menuOpen = $state(false);

  onMount(() => {
    // Layout reads page.data for breadcrumbs/TOC; clear so prior nav doesn't bleed through.
    page.clear();
  });
</script>

<article class="mx-auto max-w-4xl space-y-12 px-6 py-12">
  <header class="space-y-2">
    <h1 class="text-3xl font-semibold text-fg-default">Design Kit</h1>
    <p class="text-fg-muted">
      Dev-only showcase of every viewer primitive with its variants. Drift in tokens, sizes, or
      intents shows up next to its siblings here. No code snippets — read the primitive file or its
      tests for usage.
    </p>
  </header>

  <section class="space-y-4">
    <header class="space-y-1">
      <h2 id="buttons" class="text-2xl font-semibold text-fg-default">Buttons</h2>
      <p class="text-sm text-fg-muted">
        Variants × sizes. Use <code>iconOnly</code> for square icon buttons that need the Button states
        (loading, disabled); for purely icon-shaped chrome use IconButton.
      </p>
    </header>
    <div class="flex flex-wrap items-center gap-3">
      {#each ["primary", "secondary", "ghost", "danger"] as const as variant}
        {#each ["xs", "sm", "md"] as const as size}
          <Button {variant} {size}>{variant}/{size}</Button>
        {/each}
      {/each}
      <Button variant="primary" disabled>disabled</Button>
      <Button variant="primary" loading>loading</Button>
      <Button variant="ghost" iconOnly aria-label="iconOnly">
        <svg class="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M5 12h14" stroke-linecap="round" />
        </svg>
      </Button>
    </div>
  </section>

  <section class="space-y-4">
    <header class="space-y-1">
      <h2 id="icon-buttons" class="text-2xl font-semibold text-fg-default">Icon Buttons</h2>
      <p class="text-sm text-fg-muted">
        Square buttons with no text. Use for navigation chrome and overlay dismissals.
      </p>
    </header>
    <div class="flex flex-wrap items-center gap-3">
      <IconButton aria-label="Plus">
        <svg class="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 5v14M5 12h14" stroke-linecap="round" />
        </svg>
      </IconButton>
      <IconButton aria-label="Close">
        <svg class="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M6 6l12 12M18 6L6 18" stroke-linecap="round" />
        </svg>
      </IconButton>
      <IconButton aria-label="Search">
        <svg class="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="11" cy="11" r="7" />
          <path d="M20 20l-3.5-3.5" stroke-linecap="round" />
        </svg>
      </IconButton>
    </div>
  </section>

  <section class="space-y-4">
    <header class="space-y-1">
      <h2 id="badges" class="text-2xl font-semibold text-fg-default">Badges</h2>
      <p class="text-sm text-fg-muted">
        Pill-shaped status chips. Non-interactive — for buttons use Button.
      </p>
    </header>
    <div class="flex flex-wrap items-center gap-3">
      {#each ["neutral", "info", "warning", "attention"] as const as intent}
        {#each ["sm", "md"] as const as size}
          <Badge {intent} {size}>{intent}/{size}</Badge>
        {/each}
      {/each}
    </div>
  </section>

  <section class="space-y-4">
    <header class="space-y-1">
      <h2 id="avatars" class="text-2xl font-semibold text-fg-default">Avatars</h2>
      <p class="text-sm text-fg-muted">
        Three variants. <code>person</code> and <code>ai</code> use built-in glyphs;
        <code>initials</code> derives a two-letter label from the name prop.
      </p>
    </header>
    <div class="flex flex-wrap items-center gap-3">
      <Avatar variant="person" />
      <Avatar variant="ai" />
      <Avatar variant="initials" name="Mike Yumatov" />
    </div>
  </section>

  <section class="space-y-4">
    <header class="space-y-1">
      <h2 id="alerts" class="text-2xl font-semibold text-fg-default">Alerts</h2>
      <p class="text-sm text-fg-muted">
        Five intents. Title is optional; <code>dismissible</code> renders a close button that fires
        the <code>onDismiss</code> callback.
      </p>
    </header>
    <div class="grid max-w-2xl gap-3">
      {#each ["info", "success", "warning", "danger", "attention"] as const as intent}
        <Alert {intent}>{intent} alert body text</Alert>
      {/each}
      <Alert intent="info" title="With a title">Body text below the title.</Alert>
      <Alert intent="warning" dismissible onDismiss={() => console.debug("dismissed")}>
        Dismissible variant. Click the × to call onDismiss.
      </Alert>
    </div>
  </section>

  <section class="space-y-4">
    <header class="space-y-1">
      <h2 id="popover" class="text-2xl font-semibold text-fg-default">Popover</h2>
      <p class="text-sm text-fg-muted">
        Stateful overlay anchored to a trigger via the <code>trigger</code> snippet, or to an
        external element via <code>anchorEl</code>, or free-positioned via <code>x</code>/<code
          >y</code
        >. Spread
        <code>controlProps</code> on the trigger to wire ARIA disclosure attrs.
      </p>
    </header>
    <Popover bind:open={popoverOpen} dismissible placement="bottom" align="start">
      {#snippet trigger({ controlProps })}
        <Button variant="secondary" onclick={() => (popoverOpen = !popoverOpen)} {...controlProps}>
          Toggle popover
        </Button>
      {/snippet}
      <div
        class="
          rounded-md border border-border-default bg-bg-raised p-3 text-sm text-fg-default shadow-md
        "
      >
        Popover content. Click outside or press Escape to dismiss.
      </div>
    </Popover>
  </section>

  <section class="space-y-4">
    <header class="space-y-1">
      <h2 id="menu" class="text-2xl font-semibold text-fg-default">Menu</h2>
      <p class="text-sm text-fg-muted">
        Compound API: <code>Menu.Root</code> + <code>Menu.Item</code>. Anchor is an external
        <code>HTMLElement</code> ref. Items can be links (<code>href</code>) or buttons (<code
          >onclick</code
        >); arrow keys navigate.
      </p>
    </header>
    <div>
      <button
        type="button"
        bind:this={menuAnchor}
        onclick={() => (menuOpen = !menuOpen)}
        class="
          rounded-md border border-border-default bg-bg-raised px-3 py-1.5 text-sm text-fg-default
          hover:bg-bg-subtle
        "
      >
        Open menu
      </button>
      <Menu.Root bind:open={menuOpen} anchorEl={menuAnchor ?? null} aria-label="Demo menu">
        <Menu.Item onclick={() => console.debug("first")}>First action</Menu.Item>
        <Menu.Item onclick={() => console.debug("second")}>Second action</Menu.Item>
        <Menu.Item href="#menu">Link to anchor</Menu.Item>
      </Menu.Root>
    </div>
  </section>

  <section class="space-y-4">
    <header class="space-y-1">
      <h2 id="loading-bar" class="text-2xl font-semibold text-fg-default">LoadingBar</h2>
      <p class="text-sm text-fg-muted">
        Top-of-page progress bar. <code>threshold</code> delays appearance for fast loads; here
        forced to <code>0</code> so the trickle animation is visible. Renders position-absolute at the
        top of the nearest positioned ancestor — wrapped here in a relative container.
      </p>
    </header>
    <div class="relative h-12 rounded-md border border-border-default bg-bg-subtle">
      <LoadingBar loading threshold={0} />
    </div>
  </section>

  <section class="space-y-4">
    <header class="space-y-1">
      <h2 id="loading-skeleton" class="text-2xl font-semibold text-fg-default">LoadingSkeleton</h2>
      <p class="text-sm text-fg-muted">
        Page-loading shimmer. Fixed shape — heading bar, two paragraph blocks, and an image block —
        used while a markdown page is being fetched. Takes no props.
      </p>
    </header>
    <div class="max-w-md rounded-md border border-border-default bg-bg-default p-4">
      <LoadingSkeleton />
    </div>
  </section>

  <section class="space-y-4">
    <header class="space-y-1">
      <h2 id="quote" class="text-2xl font-semibold text-fg-default">Quote</h2>
      <p class="text-sm text-fg-muted">
        Orphan-quote blockquote with prefix/exact/suffix highlighting. Used in page-comments when
        the anchor target is no longer in the document.
      </p>
    </header>
    <div class="max-w-2xl">
      <Quote
        prefix="The kit is a "
        exact="self-contained collection of primitives"
        suffix=" that share tokens and patterns."
      />
    </div>
  </section>
</article>

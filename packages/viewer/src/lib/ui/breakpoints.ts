/**
 * Width (px) at which the layout switches from the narrow, single-column form to
 * the desktop form with a right-margin column. Mirrors the `952px` value used by
 * the `@container` queries in `Layout.svelte` — keep the two in sync (the CSS
 * cannot read this constant). Below it, inline-comment threads render in the
 * `CommentPopover` instead of the (hidden) margin aside.
 */
export const COMMENTS_BREAKPOINT_PX = 952;

/**
 * Whether the measured layout-container width is in the narrow regime. Returns
 * `false` for an unmeasured container (width 0) so the first frame defaults to
 * the desktop aside and never flashes the popover on a wide screen.
 */
export function isLayoutNarrow(width: number): boolean {
  return width > 0 && width < COMMENTS_BREAKPOINT_PX;
}

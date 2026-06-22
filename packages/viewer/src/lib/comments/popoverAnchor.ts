/** Fixed width (px) of the narrow-screen comment popover. Matches the visible
 *  thread-card width in the wide right-margin sidebar (the 320px aside minus its
 *  2rem left padding), so a thread looks the same in both surfaces. Shared by
 *  CommentPopover (renders at this width) and PageContent (clamps the popover's
 *  horizontal position against it). */
export const COMMENT_POPOVER_WIDTH_PX = 288;

/**
 * Article-relative left edge for a popover of {@link COMMENT_POPOVER_WIDTH_PX}
 * centered on `centerX`, clamped so it stays within `[margin, containerWidth -
 * width - margin]`. When the container is narrower than the popover, the left
 * pins to `margin` (the popover then runs to the edge rather than off-screen).
 */
export function clampPopoverLeft(centerX: number, containerWidth: number, margin = 8): number {
  const ideal = centerX - COMMENT_POPOVER_WIDTH_PX / 2;
  const max = Math.max(margin, containerWidth - COMMENT_POPOVER_WIDTH_PX - margin);
  return Math.min(Math.max(ideal, margin), max);
}

/** True when two ranges share interior (touching endpoints alone is not overlap). */
export function rangesOverlap(a: Range, b: Range): boolean {
  // a.end > b.start && a.start < b.end
  return (
    a.compareBoundaryPoints(Range.START_TO_END, b) > 0 &&
    a.compareBoundaryPoints(Range.END_TO_START, b) < 0
  );
}

export function rangeIntersectsNode(range: Range, node: Node): boolean {
  const nodeRange = (node.ownerDocument ?? document).createRange();
  nodeRange.selectNode(node);
  return rangesOverlap(range, nodeRange);
}

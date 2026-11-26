// Pure grid navigation, shared by every grid screen (and unit tested).

export type Dir = 'up' | 'down' | 'left' | 'right'

/** Index after moving one step in a grid of `count` items laid out in `cols` columns.
 *  Left/right wrap to the previous/next row; down from the second-to-last row lands on
 *  the last item if the row below is short. Returns the same index at the edges. */
export function step(index: number, dir: Dir, cols: number, count: number): number {
  if (count <= 0) return 0
  const i = Math.min(Math.max(index, 0), count - 1)
  switch (dir) {
    case 'left':
      return Math.max(i - 1, 0)
    case 'right':
      return Math.min(i + 1, count - 1)
    case 'up':
      return i - cols >= 0 ? i - cols : i
    case 'down': {
      if (i + cols < count) return i + cols
      // Partial last row: go to the last item if it is on the next row.
      const lastRow = Math.floor((count - 1) / cols)
      return Math.floor(i / cols) < lastRow ? count - 1 : i
    }
  }
}

/** Move by whole pages of `rows` rows (for bumpers). */
export function page(index: number, delta: number, cols: number, rows: number, count: number): number {
  if (count <= 0) return 0
  return Math.min(Math.max(index + delta * cols * rows, 0), count - 1)
}

/** Grouping letter for alphabetical jumps: A-Z, or '#' for anything else. */
export function letterOf(title: string): string {
  const t = title
    .trim()
    .replace(/^(the|a|an)\s+/i, '')
    .normalize('NFD')
  const c = t.charAt(0).toUpperCase()
  return c >= 'A' && c <= 'Z' ? c : '#'
}

/** Index of the first item of the next (delta 1) or previous (delta -1) letter group. */
export function jumpLetter(titles: string[], index: number, delta: 1 | -1): number {
  if (titles.length === 0) return 0
  const cur = letterOf(titles[index] ?? '')
  if (delta > 0) {
    for (let i = index + 1; i < titles.length; i++) if (letterOf(titles[i]) !== cur) return i
    return index
  }
  // Back: to the start of this group, or if already there, the start of the previous one.
  let i = index
  while (i > 0 && letterOf(titles[i - 1]) === cur) i--
  if (i < index) return i
  if (i === 0) return 0
  const prev = letterOf(titles[i - 1])
  while (i > 0 && letterOf(titles[i - 1]) === prev) i--
  return i
}

/** Scroll offset that keeps row `row` in view, preferring to keep it a row from the edge. */
export function scrollFor(row: number, rowHeight: number, viewport: number, current: number, total: number): number {
  const top = row * rowHeight
  const margin = rowHeight * 0.6
  let next = current
  if (top - margin < current) next = top - margin
  else if (top + rowHeight + margin > current + viewport) next = top + rowHeight + margin - viewport
  return Math.min(Math.max(next, 0), Math.max(total - viewport, 0))
}

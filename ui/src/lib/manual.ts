// Manual reader geometry (pure, unit tested).

/** Whole page on screen, or the page as wide as the screen (scrolling down it). */
export type Fit = 'page' | 'width'

/** The scale that fits a `pw`×`ph` page into a `vw`×`vh` view. */
export function fitScale(pw: number, ph: number, vw: number, vh: number, fit: Fit): number {
  if (pw <= 0 || ph <= 0) return 1
  const byWidth = vw / pw
  return fit === 'width' ? byWidth : Math.min(byWidth, vh / ph)
}

/** A page number within 1..pages. */
export function clampPage(n: number, pages: number): number {
  return Math.min(Math.max(n, 1), Math.max(pages, 1))
}

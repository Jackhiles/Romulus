/** Ease `from` toward `to` over `dt` milliseconds, independent of frame rate: a slow
 *  frame moves further instead of making the motion drag. `tau` is the time constant
 *  (about 2/3 of the remaining distance is covered in `tau` ms). */
export function follow(from: number, to: number, dt: number, tau = 55): number {
  const k = 1 - Math.exp(-Math.max(dt, 0) / tau)
  const next = from + (to - from) * k
  return Math.abs(to - next) < 0.5 ? to : next
}

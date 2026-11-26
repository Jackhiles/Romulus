export interface Toast {
  id: number
  text: string
  kind: 'info' | 'error'
}

export const toasts = $state({ list: [] as Toast[] })
let next = 1

export function toast(text: string, kind: Toast['kind'] = 'info', ms = 3500) {
  const id = next++
  toasts.list.push({ id, text, kind })
  setTimeout(() => {
    const i = toasts.list.findIndex((t) => t.id === id)
    if (i >= 0) toasts.list.splice(i, 1)
  }, ms)
}

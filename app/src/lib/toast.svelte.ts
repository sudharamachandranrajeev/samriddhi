// Short messages after an action ("Backed up."), read out by screen readers.

class Toasts {
  items = $state<{ id: number; text: string; tone: 'ok' | 'danger' }[]>([])
  #n = 0

  show(text: string, tone: 'ok' | 'danger' = 'ok') {
    const id = ++this.#n
    this.items = [...this.items, { id, text, tone }]
    setTimeout(() => (this.items = this.items.filter((t) => t.id !== id)), tone === 'danger' ? 8000 : 4000)
  }

  error(e: unknown) {
    this.show(e instanceof Error ? e.message : String(e), 'danger')
  }
}

export const toast = new Toasts()

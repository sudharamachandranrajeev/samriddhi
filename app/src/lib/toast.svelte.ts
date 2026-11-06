// Short messages after an action ("Backed up."), read out by screen readers.

class Toasts {
  items = $state<{ id: number; text: string; tone: 'ok' | 'danger' }[]>([])
  #n = 0
  /** The last three error messages, newest first, for "Report a problem". */
  #errors: string[] = []

  show(text: string, tone: 'ok' | 'danger' = 'ok') {
    const id = ++this.#n
    this.items = [...this.items, { id, text, tone }]
    setTimeout(() => (this.items = this.items.filter((t) => t.id !== id)), tone === 'danger' ? 8000 : 4000)
  }

  error(e: unknown) {
    const text = e instanceof Error ? e.message : String(e)
    this.#errors = [text, ...this.#errors].slice(0, 3)
    this.show(text, 'danger')
  }

  /**
   * Kept in memory for this run only, never written anywhere. The Rust side
   * cuts out amounts, quoted names, paths and numbers before they go into a
   * report, and the user sees the report on GitHub before sending it.
   */
  recentErrors(): string[] {
    return [...this.#errors]
  }
}

export const toast = new Toasts()

// Blended page changes: a View Transition where the platform has one, unless
// the user chose calm motion or the system asks for reduced motion.

import { tick } from 'svelte'
import { ui } from '@samriddhi/ui'

type Doc = Document & { startViewTransition?: (cb: () => Promise<void>) => unknown }

export function transition(change: () => void) {
  const doc = document as Doc
  if (ui.calm || !doc.startViewTransition) {
    change()
    return
  }
  doc.startViewTransition(async () => {
    change()
    await tick()
  })
}

/** Move focus to the new screen's heading so keyboard and screen-reader users land there. */
export async function focusHeading() {
  await tick()
  document.querySelector<HTMLElement>('main h1')?.focus({ preventScroll: false })
}

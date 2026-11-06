// Which screen to show. The shell's `status` decides first (set-up, recovery,
// lock); once the data is open, the first-run wizard runs until it is
// finished; after that the page comes from the address (#home, #wealth,
// #plan, #insights, #settings).

import type { Status } from './api.ts'

export type Page = 'home' | 'wealth' | 'plan' | 'insights' | 'settings'
export type View =
  | { kind: 'loading' }
  | { kind: 'problem'; text: string }
  | { kind: 'wizard'; resume: boolean }
  | { kind: 'recover' }
  | { kind: 'unlock' }
  | { kind: 'app'; page: Page }

export const PAGES: Page[] = ['home', 'wealth', 'plan', 'insights', 'settings']

export function pageFromHash(hash: string): Page {
  const id = hash.replace(/^#\/?/, '').split(/[/?]/)[0]
  return (PAGES as string[]).includes(id) ? (id as Page) : 'home'
}

/**
 * `setupDone` is the `setup.done` setting (null while it cannot be read:
 * before set-up or while locked).
 */
export function viewFor(status: Status | null, setupDone: boolean | null, hash: string): View {
  if (!status) return { kind: 'loading' }
  switch (status.phase) {
    case 'needsSetup':
      return { kind: 'wizard', resume: false }
    case 'needsRecovery':
      // The recovery screen also shows why (status.problem), e.g. a missing device key.
      return { kind: 'recover' }
    case 'locked':
      return { kind: 'unlock' }
    case 'open':
      if (!setupDone || status.recoveryPending) return { kind: 'wizard', resume: true }
      return { kind: 'app', page: pageFromHash(hash) }
  }
}

/** A key for page transitions: only a real change of screen animates. */
export function viewKey(v: View): string {
  return v.kind === 'app' ? `app:${v.page}` : v.kind
}

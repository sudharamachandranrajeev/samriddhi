// The step feed on the home screen: who suggested a step and whether it can
// be accepted. Accepting only notes the choice in the step log on this
// device; nothing acts on it.

import type { Step } from './api.ts'

const ORIGINS: Record<string, string> = {
  steps: 'Rules',
  'risk-profile': 'Risk profile',
  portfolio: 'Plan',
  forecast: 'Forecast',
  rebalancing: 'Rebalancing',
  'wrapper-rules': 'Tax wrappers',
  behaviour: 'Habits',
}

/** Who suggested a step, in plain words. */
export function originLabel(origin: string): string {
  return ORIGINS[origin] ?? origin
}

/** Only a suggested action that is not a question can be accepted (the core's rule too). */
export function acceptable(s: Step): boolean {
  return s.kind === 'action' && s.ask === null
}

/** True when a step's source is a web address rather than a description. */
export function isAddress(source: string): boolean {
  return /^https?:\/\//.test(source)
}

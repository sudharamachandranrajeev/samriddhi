// What the first screen says for each phase the app shell reports. The real
// screens (wizard, lock, home) come in step B; this keeps the build honest.

export type Phase = 'needsSetup' | 'needsRecovery' | 'locked' | 'open'

export interface Status {
  phase: Phase
  problem: string | null
  recoveryPending: boolean
  hasPin: boolean
  biometric: boolean
  waitSecs: number
  platform: string
  notice: string | null
}

const words: Record<Phase, string> = {
  needsSetup: 'Not set up yet on this device.',
  needsRecovery: 'Your data is here but locked to another key. Enter your recovery key.',
  locked: 'Locked. Unlock with your PIN.',
  open: 'Your data is open.',
}

export function statusText(s: Status | null): string {
  if (!s) return 'Starting…'
  return s.problem ?? words[s.phase]
}

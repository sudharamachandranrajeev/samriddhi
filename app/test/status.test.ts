import { test } from 'node:test'
import assert from 'node:assert/strict'
import { statusText, type Status } from '../src/lib/status.ts'

const base: Status = {
  phase: 'needsSetup',
  problem: null,
  recoveryPending: false,
  hasPin: false,
  biometric: false,
  waitSecs: 0,
  platform: 'linux',
  notice: null,
}

test('each phase has words, and a problem wins', () => {
  assert.equal(statusText(null), 'Starting…')
  for (const phase of ['needsSetup', 'needsRecovery', 'locked', 'open'] as const) {
    assert.ok(statusText({ ...base, phase }).endsWith('.'), phase)
  }
  assert.equal(statusText({ ...base, problem: 'The saved key is missing.' }), 'The saved key is missing.')
})

import { test } from 'node:test'
import assert from 'node:assert/strict'
import type { Status } from '../src/lib/api.ts'
import { pageFromHash, viewFor, viewKey } from '../src/lib/route.ts'
import { homeHint, HINTS } from '../src/lib/hints.ts'
import { SCREENS } from '../src/lib/wizard.ts'

const base: Status = {
  phase: 'open',
  problem: null,
  recoveryPending: false,
  hasPin: false,
  biometric: false,
  waitSecs: 0,
  platform: 'linux',
  notice: null,
}

test('the shell status picks set-up, recovery, lock or the app', () => {
  assert.deepEqual(viewFor(null, null, ''), { kind: 'loading' })
  assert.deepEqual(viewFor({ ...base, phase: 'needsSetup' }, null, ''), { kind: 'wizard', resume: false })
  assert.deepEqual(viewFor({ ...base, phase: 'needsRecovery', problem: 'The saved key is missing.' }, null, ''), { kind: 'recover' })
  assert.deepEqual(viewFor({ ...base, phase: 'locked', hasPin: true }, null, '#settings'), { kind: 'unlock' })
})

test('an open store finishes the wizard before showing pages', () => {
  assert.deepEqual(viewFor(base, false, '#home'), { kind: 'wizard', resume: true })
  assert.deepEqual(viewFor(base, null, ''), { kind: 'wizard', resume: true })
  assert.deepEqual(viewFor({ ...base, recoveryPending: true }, true, ''), { kind: 'wizard', resume: true })
  assert.deepEqual(viewFor(base, true, ''), { kind: 'app', page: 'home' })
  assert.deepEqual(viewFor(base, true, '#settings'), { kind: 'app', page: 'settings' })
  assert.deepEqual(viewFor(base, true, '#settings/answers'), { kind: 'app', page: 'settings' })
  for (const page of ['wealth', 'plan', 'insights'] as const) assert.deepEqual(viewFor(base, true, `#${page}`), { kind: 'app', page })
})

test('unknown addresses go home, and only a real change of screen animates', () => {
  assert.equal(pageFromHash(''), 'home')
  assert.equal(pageFromHash('#/settings'), 'settings')
  assert.equal(pageFromHash('#nowhere'), 'home')
  assert.equal(viewKey({ kind: 'wizard', resume: false }), viewKey({ kind: 'wizard', resume: true }))
  assert.notEqual(viewKey({ kind: 'app', page: 'home' }), viewKey({ kind: 'app', page: 'settings' }))
  assert.notEqual(viewKey({ kind: 'app', page: 'plan' }), viewKey({ kind: 'app', page: 'insights' }))
})

test('every screen has a learning hint, and the home hint is steady for a day', () => {
  for (const s of SCREENS) assert.ok(HINTS[s.id]?.text, s.id)
  for (const id of ['unlock', 'recover', 'settings', 'wealth', 'plan', 'insights']) assert.ok(HINTS[id]?.text, id)
  assert.deepEqual(homeHint('2026-11-06'), homeHint('2026-11-06'))
})

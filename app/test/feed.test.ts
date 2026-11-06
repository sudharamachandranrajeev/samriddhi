import { test } from 'node:test'
import assert from 'node:assert/strict'
import type { Step } from '../src/lib/api.ts'
import { acceptable, isAddress, originLabel } from '../src/lib/feed.ts'

const step = (extra: Partial<Step>): Step => ({
  id: 'x', origin: 'steps', kind: 'action', title: 't', why: 'w', risk: 1, effort: 'minutes', source: 'https://www.gov.uk/', asOf: '2026-11-05',
  ask: null, basis: null, disclosure: null, ...extra,
})

test('only an action that is not a question can be accepted', () => {
  assert.ok(acceptable(step({})))
  assert.ok(!acceptable(step({ kind: 'info' })))
  assert.ok(!acceptable(step({ kind: 'note' })))
  assert.ok(!acceptable(step({ ask: { fact: 'money.costsMonthly' } })))
})

test('every agent has a plain name and unknown ones show as they are', () => {
  for (const o of ['steps', 'risk-profile', 'portfolio', 'forecast', 'rebalancing', 'wrapper-rules', 'behaviour']) assert.notEqual(originLabel(o), o)
  assert.equal(originLabel('rebalancing'), 'Rebalancing')
  assert.equal(originLabel('something-new'), 'something-new')
})

test('a source is either a web address or a description', () => {
  assert.ok(isAddress('https://www.gov.uk/individual-savings-accounts'))
  assert.ok(!isAddress('Your holdings on this device'))
})

import { test } from 'node:test'
import assert from 'node:assert/strict'
import type { Holding } from '../src/lib/api.ts'
import { capFor, insightsView, ORDER, planView, rebalanceView, totals, wealthView } from '../src/lib/mock-portfolio.ts'

const h = (id: string, assetClass: Holding['assetClass'], minor: number, currency = 'GBP'): Holding => ({ id, accountId: 'a', name: id, assetClass, amount: { minor, currency } })
const holdings = [h('1', 'equity', 800_000), h('2', 'bonds', 200_000), h('3', 'cash', 5_000_000, 'INR')]
const sum = (m: Record<string, { minor: number }>) => Object.values(m).reduce((n, x) => n + x.minor, 0)

test('the preview counts only the main currency and names the rest', () => {
  const t = totals(holdings, 'GBP')
  assert.equal(t.total.minor, 1_000_000)
  assert.equal(t.weights.equity, 0.8)
  assert.equal(t.notCountedNoFx.length, 1)
  assert.deepEqual(wealthView([{ id: 'a', name: 'A', wrapper: 'isa', source: 'manual' }], holdings, 'GBP').leftOut, ['INR'])
})

test('the preview plan caps short goals unless a tier is chosen, and mixes add up to one', () => {
  const soon = [{ name: 'emergencyCushion', target: null, horizonYears: 1, horizonAssumed: false }]
  const growth = (w: Record<string, number>) => w.equity + w.property + w.gold
  const perGoal = planView({ level: 5, label: 'Very adventurous' }, soon, null).plan!
  assert.ok(growth(perGoal.blended) <= capFor(1) + 1e-9)
  const tiered = planView(null, soon, 7).plan!
  assert.ok(growth(tiered.blended) > 0.9)
  for (const p of [perGoal, tiered]) assert.ok(Math.abs(ORDER.reduce((n, c) => n + p.blended[c], 0) - 1) < 1e-9)
  assert.equal(planView(null, soon, null).plan, null)
})

test('preview trades net to zero and a contribution is spread exactly', () => {
  const target = { equity: 0.6, bonds: 0.4, property: 0, gold: 0, cash: 0 }
  const r = rebalanceView(holdings, 'GBP', target, { minor: 123_457, currency: 'GBP' })
  assert.equal(r.breached, true)
  assert.equal(sum(r.trades!), 0)
  assert.equal(sum(r.cashFlow!.flows), 123_457)
  assert.ok(r.cashFlow!.flows.bonds.minor > 0)
  assert.throws(() => rebalanceView(holdings, 'GBP', target, { minor: 100, currency: 'EUR' }))
})

test('preview insights are seeded, labelled synthetic and shaped like the shell', () => {
  const a = insightsView('GBP', null)
  const b = insightsView('GBP', null)
  assert.deepEqual(a, b)
  assert.equal(a.targetFromPlan, false)
  assert.equal(a.backtests.length, 3)
  for (const bt of a.backtests) assert.equal(bt.yearly.length, 21)
  assert.equal(a.directIndexing.indexValues.length, 61)
  assert.match(a.sampleLabel, /Synthetic/)
  assert.match(a.directIndexing.label, /Synthetic/)
})

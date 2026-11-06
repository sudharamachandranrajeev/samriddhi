import { test } from 'node:test'
import assert from 'node:assert/strict'
import type { CountryTie, RiskQuestion } from '../src/lib/api.ts'
import {
  SAFETY, SCREENS, answer, buildGoals, buildMoney, checkCountries, checkPin, checkRecoveryText, checkRisk, groupKey,
  newTie, nextScreen, prevScreen, resumeAt, setTaxHome, toggleGoal, type GoalDraft,
} from '../src/lib/wizard.ts'

test('eight screens, in the planned order, each with a one-sentence reason', () => {
  assert.deepEqual(SCREENS.map((s) => s.id), ['welcome', 'protect', 'countries', 'goals', 'money', 'risk', 'updates', 'done'])
  for (const s of SCREENS) {
    assert.ok(s.why.endsWith('.'), s.id)
    assert.equal(s.why.split(/\.\s/).length, 1, `${s.id} why is one sentence`)
  }
  assert.equal(SAFETY.length, 6)
  assert.equal(nextScreen('welcome'), 'protect')
  assert.equal(nextScreen('done'), 'done')
  assert.equal(prevScreen('welcome'), 'welcome')
  assert.equal(prevScreen('money'), 'goals')
  assert.equal(nextScreen('risk'), 'updates')
  assert.equal(prevScreen('done'), 'updates')
  assert.equal(resumeAt(true), 'protect')
  assert.equal(resumeAt(false), 'countries')
})

test('PINs: optional, digits only, 4 to 12, typed twice the same', () => {
  assert.equal(checkPin('', ''), null)
  assert.equal(checkPin('2468', '2468'), null)
  assert.match(checkPin('12a4', '12a4')!, /digits only/)
  assert.match(checkPin('123', '123')!, /4 to 12/)
  assert.match(checkPin('1234567890123', '1234567890123')!, /4 to 12/)
  assert.match(checkPin('2468', '2469')!, /different/)
})

test('the recovery key is grouped for reading and checked before sending', () => {
  const key = 'ab'.repeat(32)
  assert.equal(groupKey(key).split(' ').length, 8)
  assert.equal(checkRecoveryText(groupKey(key)), null)
  assert.equal(checkRecoveryText(key.toUpperCase()), null)
  assert.match(checkRecoveryText('')!, /Type or paste/)
  assert.match(checkRecoveryText('zz')!, /0-9/)
  assert.match(checkRecoveryText('ab12')!, /this has 4/)
})

test('countries: one home, one tax home, every country tied somehow', () => {
  const gb = newTie('GB', true)
  const ind = newTie('IN', false)
  assert.equal(checkCountries([]), 'Add at least one country.')
  assert.match(checkCountries([{ ...gb, residence: false }])!, /live in/)
  assert.match(checkCountries([gb, ind])!, /at least one link/)
  const both = setTaxHome([gb, { ...ind, citizen: true }], 'IN', true)
  assert.deepEqual(both.map((c) => c.taxHome), [false, true])
  assert.equal(checkCountries(both), null)
  const two: CountryTie[] = [gb, { ...ind, taxHome: true, citizen: true }]
  assert.match(checkCountries(two)!, /Only one/)
})

test('goals: at most three, toggled, optional targets in exact money, future dates', () => {
  let g: GoalDraft[] = []
  for (const k of ['emergencyCushion', 'clearDebt', 'buyHome', 'retireEarly'] as const) g = toggleGoal(g, k)
  assert.deepEqual(g.map((x) => x.kind), ['emergencyCushion', 'clearDebt', 'buyHome'])
  g = toggleGoal(g, 'clearDebt')
  assert.deepEqual(g.map((x) => x.kind), ['emergencyCushion', 'buyHome'])
  g[0].target = '13,000'
  g[0].targetDate = '2027-12-31'
  const ok = buildGoals(g, 'GBP', '2026-11-06')
  assert.ok('goals' in ok)
  assert.deepEqual(ok.goals[0], { kind: 'emergencyCushion', target: { minor: 1300000, currency: 'GBP' }, targetDate: '2027-12-31' })
  assert.deepEqual(ok.goals[1], { kind: 'buyHome', target: null, targetDate: null })
  g[0].targetDate = '2026-01-01'
  assert.match((buildGoals(g, 'GBP', '2026-11-06') as { error: string }).error, /future/)
  g[0].targetDate = ''
  g[0].target = '1.234'
  assert.match((buildGoals(g, 'GBP', '2026-11-06') as { error: string }).error, /Emergency cushion: Use at most 2/)
})

test('money screen: empty boxes are skipped, debts need a name and an amount', () => {
  const base = { currency: 'GBP', takeHome: '3,100', costs: '', cash: '4200', debts: null }
  const ok = buildMoney(base)
  assert.ok('facts' in ok)
  assert.deepEqual(ok.facts, { takeHome: { minor: 310000, currency: 'GBP' }, costs: null, cash: { minor: 420000, currency: 'GBP' }, debts: null })
  const none = buildMoney({ ...base, debts: [] })
  assert.ok('facts' in none && Array.isArray(none.facts.debts) && none.facts.debts.length === 0)
  const card = buildMoney({ ...base, debts: [{ label: ' Credit card ', balance: '1,800', band: 'over20' }] })
  assert.ok('facts' in card)
  assert.deepEqual(card.facts.debts, [{ label: 'Credit card', balance: { minor: 180000, currency: 'GBP' }, band: 'over20' }])
  assert.match((buildMoney({ ...base, debts: [{ label: '', balance: '1', band: 'unknown' }] }) as { error: string }).error, /short name/)
  assert.match((buildMoney({ ...base, debts: [{ label: 'Loan', balance: '', band: 'unknown' }] }) as { error: string }).error, /how much/)
  assert.match((buildMoney({ ...base, cash: '-1' }) as { error: string }).error, /^Cash savings: /)
})

test('risk: attitude and capacity are counted apart until every question is answered', () => {
  const q = (id: string, part: 'attitude' | 'capacity'): RiskQuestion => ({ id, part, text: id, why: '', options: [{ id: 'x', text: 'x', score: 1 }] })
  const qs = [q('a1', 'attitude'), q('a2', 'attitude'), q('c1', 'capacity')]
  let a = answer([], 'a1', 'x')
  a = answer(a, 'a1', 'y')
  assert.equal(a.length, 1)
  assert.equal(a[0].optionId, 'y')
  assert.equal(checkRisk(qs, a), 'Answer the rest: 1 about how you feel and 1 about what you could afford to lose.')
  a = answer(answer(a, 'a2', 'x'), 'c1', 'x')
  assert.equal(checkRisk(qs, a), null)
})

import { test } from 'node:test'
import assert from 'node:assert/strict'
import { currencyFor, exponent, formatMoney, monthsCovered, parseAmount, roughNetWorth, sumMoney, toInput } from '../src/lib/money.ts'

const gbp = (minor: number) => ({ minor, currency: 'GBP' })

test('typed amounts become exact minor units, without floating point', () => {
  const ok = (text: string, cur: string) => {
    const p = parseAmount(text, cur)
    assert.ok(p.ok, `${text}: ${!p.ok && p.error}`)
    return p.money.minor
  }
  assert.equal(ok('1,250', 'GBP'), 125000)
  assert.equal(ok('1250.5', 'GBP'), 125050)
  assert.equal(ok('0.1', 'GBP'), 10)
  assert.equal(ok('0.29', 'GBP'), 29) // 0.29 * 100 is 28.999… as a float
  assert.equal(ok('£ 1 234.56', 'GBP'), 123456)
  assert.equal(ok('1,23,456', 'INR'), 12345600)
  assert.equal(ok('AED 3000', 'AED'), 300000)
  assert.equal(ok('1500', 'JPY'), 1500)
  assert.equal(ok('1.234', 'KWD'), 1234)
  assert.equal(ok('.5', 'GBP'), 50)
  assert.equal(parseAmount('1250', 'gbp').ok && parseAmount('1250', 'gbp').money.currency, 'GBP')
})

test('odd amounts are refused with a plain reason, never rounded', () => {
  for (const [text, cur, words] of [
    ['', 'GBP', 'Type an amount'],
    ['-5', 'GBP', 'below zero'],
    ['12.345', 'GBP', 'at most 2 digits'],
    ['10.5', 'JPY', 'no smaller unit'],
    ['ten', 'GBP', 'digits only'],
    ['1.2.3', 'GBP', 'digits only'],
    ['99999999999999999', 'GBP', 'too large'],
  ] as const) {
    const p = parseAmount(text, cur)
    assert.equal(p.ok, false, text)
    assert.ok(!p.ok && p.error.includes(words), `${text}: ${!p.ok && p.error}`)
  }
})

test('money is formatted from minor units with the right symbol and grouping', () => {
  assert.equal(formatMoney(gbp(1240000)), '£12,400')
  assert.equal(formatMoney(gbp(125050)), '£1,250.50')
  assert.equal(formatMoney(gbp(125000), { trim: false }), '£1,250.00')
  assert.equal(formatMoney(gbp(5)), '£0.05')
  assert.equal(formatMoney(gbp(-25000)), '-£250')
  assert.equal(formatMoney({ minor: 12345650, currency: 'INR' }), '₹1,23,456.50')
  assert.equal(formatMoney({ minor: 1000000000, currency: 'INR' }), '₹1,00,00,000')
  assert.equal(formatMoney({ minor: 300000, currency: 'AED' }), 'AED 3,000')
  assert.equal(formatMoney({ minor: 1500, currency: 'JPY' }), '¥1,500')
  assert.equal(formatMoney({ minor: 1234, currency: 'KWD' }), '1.234 KWD')
  assert.equal(formatMoney({ minor: 1.5, currency: 'GBP' }), '—')
})

test('an amount goes back into a box the way it was typed', () => {
  assert.equal(toInput(gbp(125050)), '1250.50')
  assert.equal(toInput(gbp(1240000)), '12400')
  assert.equal(toInput({ minor: 300000, currency: 'AED' }), '3000')
  assert.equal(toInput({ minor: 1234, currency: 'KWD' }), '1.234')
  assert.equal(toInput(null), '')
  const back = parseAmount(toInput({ minor: 12345650, currency: 'INR' }), 'INR')
  assert.ok(back.ok && back.money.minor === 12345650)
})

test('rough net worth is cash minus debts, only when both are known and in one currency', () => {
  assert.deepEqual(roughNetWorth(gbp(420000), [{ balance: gbp(180000) }, { balance: gbp(640000) }]), gbp(-400000))
  assert.deepEqual(roughNetWorth(gbp(420000), []), gbp(420000))
  assert.equal(roughNetWorth(null, []), null)
  assert.equal(roughNetWorth(gbp(1), null), null)
  assert.equal(roughNetWorth(gbp(1), [{ balance: { minor: 1, currency: 'INR' } }]), null)
})

test('sums, cushion months and helpers', () => {
  assert.deepEqual(sumMoney([gbp(100), gbp(250)]), gbp(350))
  assert.equal(sumMoney([]), null)
  assert.equal(sumMoney([gbp(1), { minor: 1, currency: 'AED' }]), null)
  assert.equal(monthsCovered(gbp(420000), gbp(215000)), '1.9') // rounded down, as the core does
  assert.equal(monthsCovered(gbp(645000), gbp(215000)), '3.0')
  assert.equal(monthsCovered(gbp(420000), gbp(0)), null)
  assert.equal(monthsCovered(gbp(420000), null), null)
  assert.equal(exponent('BHD'), 3)
  assert.equal(exponent('GBP'), 2)
  assert.equal(currencyFor('IN'), 'INR')
  assert.equal(currencyFor('AE'), 'AED')
  assert.equal(currencyFor('SG'), 'GBP')
})

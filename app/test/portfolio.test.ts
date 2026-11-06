import { test } from 'node:test'
import assert from 'node:assert/strict'
import { CLASSES, classLabel, goalName, growthShare, horizonText, linePoints, mixText, pct, points, tierText, wrapperLabel } from '../src/lib/portfolio.ts'

test('percentages and drift read plainly and never show minus zero', () => {
  assert.equal(pct(0.4512), '45%')
  assert.equal(pct(0.4512, 1), '45.1%')
  assert.equal(pct(-0.0001), '0%')
  assert.equal(pct(Number.NaN), '—')
  assert.equal(points(0.021), '+2.1 pts')
  assert.equal(points(-0.004), '-0.4 pts')
  assert.equal(points(-0.0001), '0.0 pts')
})

test('every asset class has a name and a plain explanation', () => {
  assert.equal(CLASSES.length, 5)
  for (const c of CLASSES) assert.ok(c.label && c.explain.length > 20, c.id)
  assert.equal(classLabel('property'), 'Property / REITs')
  assert.equal(wrapperLabel('isa'), 'ISA')
})

test('a mix lists the classes that matter, in class order', () => {
  assert.equal(mixText({ cash: 0.15, equity: 0.45, bonds: 0.4, gold: 0.001 }), 'Equities 45%, Bonds 40%, Cash 15%')
  assert.equal(mixText({}), 'Nothing yet')
  assert.ok(Math.abs(growthShare({ equity: 0.5, property: 0.1, gold: 0.05, bonds: 0.35 }) - 0.65) < 1e-9)
})

test('goal names and horizons are in plain words', () => {
  assert.equal(goalName('buyHome'), 'Buy a home')
  assert.equal(goalName('General investing'), 'General investing')
  assert.equal(horizonText(0), 'now')
  assert.equal(horizonText(0.5), 'under a year')
  assert.equal(horizonText(1.2), 'about 1 year')
  assert.equal(horizonText(23.15), 'about 23 years')
})

test('chart points span the box with the lowest value at the bottom', () => {
  assert.equal(linePoints([], 100, 40), '')
  assert.equal(linePoints([1, 3, 2], 100, 40), '0,40 50,0 100,20')
  assert.equal(linePoints([5, 5], 100, 40), '0,40 100,40', 'a flat line does not divide by zero')
  assert.equal(linePoints([2], 100, 40, 0, 4), '0,20', 'a shared scale')
})

test('tier mode says it is one fixed mix for every goal', () => {
  assert.match(tierText(null, 7), /^Per goal/)
  assert.match(tierText(5, 7), /Tier 5 of 7.*one fixed mix for every goal/)
})

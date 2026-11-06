import { test } from 'node:test'
import assert from 'node:assert/strict'
import { tokens, defaultTheme } from '../src/generated/tokens.ts'
import { check, ratio } from '../src/contrast.ts'
import { colourNames, themeVars } from '../src/compile.ts'
import { validateTheme } from '../src/presets.ts'

test('contrast ratio matches WCAG reference values', () => {
  assert.equal(Math.round(ratio('#000000', '#ffffff') * 100) / 100, 21)
  assert.equal(ratio('#777777', '#777777'), 1)
  assert.ok(Math.abs(ratio('#767676', '#ffffff') - 4.54) < 0.01)
})

test('every built-in theme has light and dark colours that pass every contrast pair', () => {
  for (const [id, t] of Object.entries(tokens.themes)) {
    for (const mode of ['light', 'dark'] as const) {
      const c = t.modes[mode]
      assert.ok(c, `${id} lacks ${mode}`)
      for (const n of colourNames) assert.match(c[n], /^#[0-9a-f]{6}$/i, `${id}/${mode}.${n}`)
      assert.deepEqual(check(c, tokens.contrast.pairs), [], `${id}/${mode}`)
    }
  }
})

test('theme variables resolve colour references and leave no braces behind', () => {
  for (const t of Object.values(tokens.themes)) {
    const v = themeVars(tokens.fonts, t, 'light')
    for (const [k, val] of Object.entries(v)) assert.ok(!val.includes('{'), `${k} = ${val}`)
    assert.match(v['--card-border'], /^(0|1px solid var\(--c-line\))$/)
  }
})

const swarna = () => JSON.parse(JSON.stringify(tokens.themes.swarna))

test('every exported built-in theme imports back unchanged', () => {
  for (const [id, t] of Object.entries(tokens.themes)) {
    const copy = JSON.parse(JSON.stringify(t))
    assert.deepEqual(validateTheme(copy), copy, id)
  }
})

test('Swarna is the default, and Swarna, Bahi and Vana all ship', () => {
  assert.equal(defaultTheme, 'swarna')
  assert.deepEqual(Object.keys(tokens.themes), ['swarna', 'bahi', 'vana'])
})

test('imported themes cannot carry arbitrary CSS', () => {
  const evil = swarna()
  evil.style.cardShadow = 'none; } body { display: none'
  assert.equal(typeof validateTheme(evil), 'string')
  const badColour = swarna()
  badColour.modes.light.ink = 'red; background: url(https://example.com/x)'
  assert.equal(typeof validateTheme(badColour), 'string')
  const badFont = swarna()
  badFont.type.body = 'Comic Sans'
  assert.equal(typeof validateTheme(badFont), 'string')
})

test('style looks can be mixed across built-in themes', () => {
  const mix = swarna()
  for (const k of ['rowBg', 'rowRadius', 'rowDivider'] as const) mix.style[k] = tokens.themes.bahi.style[k]
  assert.equal(typeof validateTheme(mix), 'object')
})

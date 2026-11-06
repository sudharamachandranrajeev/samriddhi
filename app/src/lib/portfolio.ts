// Words and small formatting for the Wealth, Plan and Insights screens:
// asset-class and account names, percentages, horizons, and the points of
// the small line charts. The numbers themselves come from the Rust core.

import type { AssetClass, ClassWeights, Wrapper } from './api.ts'
import { GOALS } from './wizard.ts'

export const CLASSES: { id: AssetClass; label: string; explain: string }[] = [
  { id: 'equity', label: 'Equities', explain: 'Shares in companies, usually through funds. They grow most over long periods and fall the most in bad years.' },
  { id: 'bonds', label: 'Bonds', explain: 'Loans to governments and companies that pay interest. Steadier than shares, and they often hold up when shares fall.' },
  { id: 'property', label: 'Property / REITs', explain: 'Property funds and real estate investment trusts, not your own home. They earn rent and can be slow to sell.' },
  { id: 'gold', label: 'Gold / commodities', explain: 'Gold and other raw materials. They earn nothing themselves but can hold value when other things fall.' },
  { id: 'cash', label: 'Cash', explain: 'Bank and savings accounts and money-market funds. Safe from falls, but it can lose value to rising prices.' },
]

export const classLabel = (c: AssetClass) => CLASSES.find((x) => x.id === c)?.label ?? c

export const WRAPPERS: { value: Wrapper; label: string }[] = [
  { value: 'isa', label: 'ISA' },
  { value: 'pension', label: 'Pension' },
  { value: 'taxable', label: 'Taxable account' },
  { value: 'cash', label: 'Cash account' },
]

export const wrapperLabel = (w: Wrapper) => WRAPPERS.find((x) => x.value === w)?.label ?? w

/** 0.4512 -> "45%"; with digits, "45.1%". Never "-0%". */
export function pct(w: number, digits = 0): string {
  if (!Number.isFinite(w)) return '—'
  const s = (w * 100).toFixed(digits)
  return `${/^-0(\.0+)?$/.test(s) ? s.slice(1) : s}%`
}

/** A drift in percentage points: +2.1 pts, -0.4 pts, 0.0 pts. */
export function points(d: number): string {
  if (!Number.isFinite(d)) return '—'
  const s = (Math.abs(d) * 100).toFixed(1)
  const sign = s === '0.0' ? '' : d > 0 ? '+' : '-'
  return `${sign}${s} pts`
}

/** A goal's name as the plan engine gives it (a goal kind) in plain words. */
export function goalName(name: string): string {
  return GOALS.find((g) => g.kind === name)?.label ?? name
}

/** How far off a goal is: "now", "under a year", "about 1 year", "about 23 years". */
export function horizonText(years: number): string {
  if (!(years > 0.05)) return 'now'
  if (years < 1) return 'under a year'
  const n = Math.round(years)
  return `about ${n} year${n === 1 ? '' : 's'}`
}

/** "Equities 45%, Bonds 40%, Cash 15%": every class at half a percent or more, in class order. */
export function mixText(w: Partial<ClassWeights>): string {
  const parts = CLASSES.filter((c) => (w[c.id] ?? 0) >= 0.005).map((c) => `${c.label} ${pct(w[c.id] ?? 0)}`)
  return parts.length ? parts.join(', ') : 'Nothing yet'
}

/** The growth share of a mix: equities, property and gold. */
export function growthShare(w: Partial<ClassWeights>): number {
  return (w.equity ?? 0) + (w.property ?? 0) + (w.gold ?? 0)
}

/**
 * Points for an SVG polyline in a `width` x `height` box, the lowest value
 * at the bottom. `lo`/`hi` share a scale between several lines.
 */
export function linePoints(values: number[], width: number, height: number, lo?: number, hi?: number): string {
  if (!values.length) return ''
  const min = lo ?? Math.min(...values)
  const max = hi ?? Math.max(...values)
  const span = max - min || 1
  const step = values.length > 1 ? width / (values.length - 1) : 0
  return values
    .map((v, i) => `${round(i * step)},${round(height - ((v - min) / span) * height)}`)
    .join(' ')
}

const round = (n: number) => Math.round(n * 10) / 10

/** Tier mode in words, for the plan screen. */
export function tierText(tier: number | null, count: number): string {
  if (tier === null) return 'Per goal: each goal moves to safer assets as its date nears.'
  return `Tier ${tier} of ${count} (1 cautious, ${count} adventurous): one fixed mix for every goal.`
}

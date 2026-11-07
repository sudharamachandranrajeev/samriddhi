// Made-up answers for the wealth, plan and insights commands, for the browser
// preview and the screenshot harness (see mock.ts). Shaped like the shell's
// answers; the real rules (the optimiser, the forecasts, the backtests) live
// in the Rust core. Every figure here is synthetic and seeded, so the
// screenshots come out the same each run.

import type {
  Account, AccountView, AssetClass, BacktestView, ClassWeights, DirectIndexView, Forecast, GoalInput, Holding, InsightsView,
  PlanResult, PlanView, RebalancePlan, RebalanceView, TargetAllocation, Totals, WealthView,
} from './api.ts'
import type { Money } from './money.ts'

export const ORDER: AssetClass[] = ['equity', 'bonds', 'property', 'gold', 'cash']
const ASSUME: Record<AssetClass, [number, number]> = {
  equity: [0.07, 0.16], bonds: [0.035, 0.06], property: [0.06, 0.14], gold: [0.03, 0.15], cash: [0.02, 0.005],
}
export const DISCLOSURE = 'This is a model using illustrative assumptions, not financial advice or a guarantee. Past and simulated results do not predict future returns.'
const AS_OF = '2026-11-05'
const SOURCE = 'Generic long-run illustrative figures. Synthetic: not market data and not a forecast.'
const SAMPLE = 'Synthetic sample data (seeded random, not real market history).'
const TIERS = [0.1, 0.25, 0.4, 0.55, 0.7, 0.85, 0.95]
const GROWTH_BY_LEVEL = [0.2, 0.35, 0.5, 0.65, 0.8]
const BAND = 0.03

const money = (minor: number, currency: string): Money => ({ minor: Math.round(minor), currency })
const each = (f: (c: AssetClass) => number): ClassWeights => Object.fromEntries(ORDER.map((c) => [c, f(c)])) as ClassWeights
const eachMoney = (f: (c: AssetClass) => number, currency: string) => Object.fromEntries(ORDER.map((c) => [c, money(f(c), currency)])) as Record<AssetClass, Money>

/** A small seeded generator (mulberry32), so every run draws the same numbers. */
function rng(seed: number) {
  let a = seed >>> 0
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
  const normal = () => Math.sqrt(-2 * Math.log(1 - next())) * Math.cos(2 * Math.PI * next())
  return { next, normal }
}

// ---- Wealth ----

export function totals(holdings: Holding[], base: string): Totals {
  const counted = holdings.filter((h) => h.amount.currency === base)
  const sum = (c: AssetClass) => counted.filter((h) => h.assetClass === c).reduce((n, h) => n + h.amount.minor, 0)
  const total = ORDER.reduce((n, c) => n + sum(c), 0)
  return {
    base,
    total: money(total, base),
    byClass: eachMoney(sum, base),
    weights: each((c) => (total > 0 ? sum(c) / total : 0)),
    notCountedNoFx: holdings.filter((h) => h.amount.currency !== base),
  }
}

export function wealthView(accounts: Account[], holdings: Holding[], base: string): WealthView {
  const t = totals(holdings, base)
  const views: AccountView[] = accounts.map((account) => {
    const hs = holdings.filter((h) => h.accountId === account.id)
    return {
      account,
      holdings: hs,
      counted: money(hs.filter((h) => h.amount.currency === base).reduce((n, h) => n + h.amount.minor, 0), base),
      notCounted: hs.filter((h) => h.amount.currency !== base).length,
    }
  })
  return { base, accounts: views, totals: t, leftOut: [...new Set(t.notCountedNoFx.map((h) => h.amount.currency))].sort() }
}

// ---- Plan ----

/** The most a goal may hold in growth assets, by how soon it is needed. */
export const capFor = (years: number) => (years <= 2 ? 0.1 : years <= 5 ? 0.4 : years <= 10 ? 0.7 : 0.9)

function mix(growth: number): ClassWeights {
  const safe = 1 - growth
  return { equity: growth * 0.8, bonds: safe * 0.75, property: growth * 0.1, gold: growth * 0.1, cash: safe * 0.25 }
}

function allocation(weights: ClassWeights, risk: number, years: number): TargetAllocation {
  const ret = ORDER.reduce((n, c) => n + weights[c] * ASSUME[c][0], 0)
  // A rough stand-in for the optimiser's covariance: equity and property move together.
  const v = (c: AssetClass) => weights[c] * ASSUME[c][1]
  const variance = ORDER.reduce((n, c) => n + v(c) ** 2, 0) + 2 * 0.7 * v('equity') * v('property')
  return { weights, expectedReturn: ret, volatility: Math.sqrt(variance), riskScore: risk, horizonYears: years }
}

export function planView(risk: { level: number; label: string } | null, goals: GoalInput[], tier: number | null): PlanView {
  const view = { tierCount: TIERS.length, assumptions: ORDER.map((c) => ({ class: c, expectedReturn: ASSUME[c][0], volatility: ASSUME[c][1] })), assumptionsNote: SOURCE, disclosure: DISCLOSURE }
  const all = goals.length ? goals : [{ name: 'General investing', target: null, horizonYears: 20, horizonAssumed: true }]
  if (!risk && tier === null) return { ...view, plan: null, caps: [], tier, riskLevel: null, riskLabel: null }
  const perGoal = all.map((goal) => {
    const growth = tier !== null ? TIERS[tier - 1] : Math.min(GROWTH_BY_LEVEL[(risk?.level ?? 3) - 1], capFor(goal.horizonYears))
    return { goal, allocation: allocation(mix(growth), Math.round(growth * 100), goal.horizonYears) }
  })
  const byTarget = all.every((g) => g.target && g.target.minor > 0)
  const share = all.map((g) => (byTarget ? g.target!.minor : 1))
  const whole = share.reduce((a, b) => a + b, 0)
  const blended = each((c) => perGoal.reduce((n, p, i) => n + p.allocation.weights[c] * (share[i] / whole), 0))
  const plan: PlanResult = { perGoal, blended, tier, assumptionsAsOf: AS_OF, assumptionsSource: SOURCE }
  return { ...view, plan, caps: all.map((g) => capFor(g.horizonYears)), tier, riskLevel: risk?.level ?? null, riskLabel: risk?.label ?? null }
}

// ---- Rebalancing ----

/** Round each class's share of `minor`, putting the rounding left over on the largest. */
function spread(parts: ClassWeights, minor: number, currency: string): Record<AssetClass, Money> {
  const raw = each((c) => Math.round(parts[c] * minor))
  const gap = minor - ORDER.reduce((n, c) => n + raw[c], 0)
  const big = ORDER.reduce((a, c) => (Math.abs(raw[c]) > Math.abs(raw[a]) ? c : a), 'equity' as AssetClass)
  raw[big] += gap
  return eachMoney((c) => raw[c], currency)
}

export function rebalanceView(holdings: Holding[], base: string, target: ClassWeights | null, cashFlow: Money | null): RebalanceView {
  if (cashFlow && cashFlow.currency !== base) throw `Give the amount in ${base}, the currency your totals are in.`
  const t = totals(holdings, base)
  const total = t.total.minor
  const goal = target ?? t.weights
  const rows = ORDER.map((c) => ({ assetClass: c, target: goal[c], current: t.weights[c], drift: t.weights[c] - goal[c], value: t.byClass[c] }))
  const breached = total > 0 && rows.some((r) => Math.abs(r.drift) > BAND + 1e-9)
  let flow: RebalancePlan | null = null
  if (cashFlow && total > 0) {
    const after = total + cashFlow.minor
    if (after < 0) throw 'That is more than your counted holdings.'
    const gapOf = (c: AssetClass) => (cashFlow.minor >= 0 ? goal[c] * after - t.byClass[c].minor : t.byClass[c].minor - goal[c] * after)
    const gaps = each((c) => Math.max(0, gapOf(c)))
    const sumGaps = ORDER.reduce((n, c) => n + gaps[c], 0)
    const size = Math.abs(cashFlow.minor)
    const parts = each((c) => (sumGaps > 0 ? (gaps[c] / sumGaps) * Math.min(1, sumGaps / size) : 0) + (sumGaps < size ? goal[c] * (1 - sumGaps / size) : 0))
    const flows = spread(parts, cashFlow.minor, base)
    const afterWeights = each((c) => (after > 0 ? (t.byClass[c].minor + flows[c].minor) / after : 0))
    flow = { flows, afterWeights, stillBreached: ORDER.some((c) => Math.abs(afterWeights[c] - goal[c]) > BAND + 1e-9) }
  }
  // The sell-based option: every class back to target, netting to zero.
  const trades = breached && target ? eachMoney((c) => goal[c] * total - t.byClass[c].minor, base) : null
  if (trades) trades.cash = money(trades.cash.minor - ORDER.reduce((n, c) => n + trades[c].minor, 0), base)
  return {
    base, total: t.total, hasPlan: target !== null, rows, bands: { over: BAND, under: BAND }, breached,
    leftOut: t.notCountedNoFx.length, leftOutCurrencies: [...new Set(t.notCountedNoFx.map((h) => h.amount.currency))].sort(),
    cashFlow: flow, trades, disclosure: DISCLOSURE,
  }
}

// ---- Insights ----

function forecast(history: number[]): Forecast {
  const alpha = 0.3
  let level = history[0]
  const errors: number[] = []
  for (const r of history.slice(1)) {
    errors.push(r - level)
    level = alpha * r + (1 - alpha) * level
  }
  const sd = Math.sqrt(errors.reduce((n, e) => n + e * e, 0) / errors.length)
  const half = 1.2816 * sd
  const trials = errors.length - 4
  const hits = errors.slice(4).filter((e) => Math.abs(e) <= half).length
  const mae = errors.slice(4).reduce((n, e) => n + Math.abs(e), 0) / trials
  const naive = history.slice(5).reduce((n, r, i) => n + Math.abs(r - history[i + 4]), 0) / trials
  return { mean: [level], lower: [level - half], upper: [level + half], confidence: 0.8, alpha, track: { trials, hits, maeModel: mae, maeNaive: naive } }
}

function backtest(rule: BacktestView['rule'], label: string, monthly: Record<AssetClass, number[]>, target: ClassWeights, start: Money): BacktestView {
  let pots = each((c) => target[c] * start.minor)
  const sum = () => ORDER.reduce((n, c) => n + pots[c], 0)
  const values = [start.minor]
  let rebalances = 0
  const months = monthly.equity.length
  for (let m = 0; m < months; m++) {
    pots = each((c) => pots[c] * (1 + monthly[c][m]))
    const v = sum()
    const off = ORDER.some((c) => Math.abs(pots[c] / v - target[c]) > BAND)
    if ((rule === 'annual' && (m + 1) % 12 === 0) || (rule === 'threshold' && off)) {
      pots = each((c) => target[c] * v)
      rebalances++
    }
    values.push(v)
  }
  const rets = values.slice(1).map((v, i) => v / values[i] - 1)
  const avg = rets.reduce((a, b) => a + b, 0) / rets.length
  const vol = Math.sqrt(rets.reduce((n, r) => n + (r - avg) ** 2, 0) / (rets.length - 1)) * Math.sqrt(12)
  let peak = values[0]
  let worst = 0
  for (const v of values) {
    peak = Math.max(peak, v)
    worst = Math.min(worst, v / peak - 1)
  }
  const final = values[values.length - 1]
  return {
    rule, label, yearly: values.filter((_, i) => i % 12 === 0).map((v) => money(v, start.currency)), finalValue: money(final, start.currency),
    cagr: (final / start.minor) ** (12 / months) - 1, annualVol: vol, maxDrawdown: worst, rebalances,
  }
}

function directIndexing(base: string): DirectIndexView {
  const r = rng(11)
  const start = 1_000_000
  const index = [start]
  const portfolio = [start]
  for (let m = 0; m < 60; m++) {
    const step = 0.006 + 0.045 * r.normal()
    index.push(index[m] * (1 + step))
    portfolio.push(portfolio[m] * (1 + step + 0.002 * r.normal()))
  }
  const losses = 186_400
  const diffs = portfolio.slice(1).map((v, i) => v / portfolio[i] - index[i + 1] / index[i])
  const tracking = Math.sqrt(diffs.reduce((n, d) => n + d * d, 0) / diffs.length) * Math.sqrt(12)
  return {
    stocks: 40, months: 60, harvestThreshold: 0.1, taxRate: 0.2,
    indexValues: index.map((v) => money(v, base)), portfolioValues: portfolio.map((v) => money(v, base)),
    harvestedLosses: money(losses, base), harvests: 14, taxOffsetValue: money(losses * 0.2, base), trackingDifference: tracking,
    label: 'Synthetic constituents, for illustration only, not real market history.',
  }
}

export function insightsView(base: string, planTarget: ClassWeights | null): InsightsView {
  const r = rng(7)
  const monthly = each(() => 0) as unknown as Record<AssetClass, number[]>
  for (const c of ORDER) monthly[c] = []
  for (let m = 0; m < 240; m++) {
    const market = r.normal()
    for (const c of ORDER) {
      const [mu, vol] = ASSUME[c]
      const shared = c === 'equity' || c === 'property' ? 0.8 * market + 0.6 * r.normal() : r.normal()
      monthly[c].push(mu / 12 + (vol / Math.sqrt(12)) * shared)
    }
  }
  const yearly = (c: AssetClass) => Array.from({ length: 20 }, (_, y) => monthly[c].slice(y * 12, y * 12 + 12).reduce((n, x) => n * (1 + x), 1) - 1)
  const target = planTarget ?? { equity: 0.6, bonds: 0.4, property: 0, gold: 0, cash: 0 }
  const start = money(1_000_000, base)
  return {
    base,
    forecasts: ORDER.filter((c) => c !== 'cash').map((c) => ({ assetClass: c, history: yearly(c), forecast: forecast(yearly(c)) })),
    backtestTarget: target,
    targetFromPlan: planTarget !== null,
    start,
    backtests: [
      backtest('none', 'Never rebalance', monthly, target, start),
      backtest('annual', 'Rebalance every 12 months', monthly, target, start),
      backtest('threshold', 'Rebalance when a band is breached', monthly, target, start),
    ],
    directIndexing: directIndexing(base),
    sampleLabel: SAMPLE,
    assumptionsAsOf: AS_OF,
    disclosure: DISCLOSURE,
  }
}

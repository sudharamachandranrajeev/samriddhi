// The first-run wizard as data and pure checks, so the rules can be tested
// without a screen. Seven screens; each says in one sentence why it asks.

import type { CountryTie, Debt, Goal, GoalKind, RateBand, RiskAnswer, RiskQuestion } from './api.ts'
import { parseAmount, type Money } from './money.ts'

export type ScreenId = 'welcome' | 'protect' | 'countries' | 'goals' | 'money' | 'risk' | 'done'

export const SCREENS: { id: ScreenId; title: string; why: string }[] = [
  {
    id: 'welcome',
    title: 'Welcome to Samriddhi',
    why: 'Before anything else, here is what Samriddhi will and will not do, so you can decide if it suits you.',
  },
  {
    id: 'protect',
    title: 'Protect your data',
    why: 'Your data is locked with a key only this device holds, so you need a spare key in case the device is lost.',
  },
  {
    id: 'countries',
    title: 'Your countries',
    why: 'Tax rules, accounts and allowances depend on the country, so we only suggest what fits yours.',
  },
  {
    id: 'goals',
    title: 'What you want most',
    why: 'Knowing what matters to you lets us put the steps that help it first.',
  },
  {
    id: 'money',
    title: 'Money in and out',
    why: 'A rough picture of what comes in, goes out and is owed shows which step would help most right now.',
  },
  {
    id: 'risk',
    title: 'How you feel about risk',
    why: 'We never suggest a move that is riskier than you are willing, and able, to take.',
  },
  {
    id: 'done',
    title: 'Your first step',
    why: 'One clear next step is easier to act on than a long list.',
  },
]

/** The six safety points shown and accepted on the first screen (from the plan). */
export const SAFETY: { title: string; text: string }[] = [
  {
    title: 'Guidance, not regulated advice',
    text: 'Samriddhi explains and suggests; it is not regulated financial advice. Big or hard-to-undo moves (pension transfers, large lump sums, property, borrowing to invest) say "check with a regulated adviser" and show how to find one on the FCA register.',
  },
  {
    title: 'No promised returns',
    text: 'Every suggestion shows what could go wrong, the worst realistic case, how sure we are and where the facts came from. Anything offering guaranteed or unusually high returns is flagged as a likely scam.',
  },
  {
    title: 'Never moves your money',
    text: 'Samriddhi never moves money, places trades or makes payments. You stay in charge of every action.',
  },
  {
    title: 'People data stays with you',
    text: 'Anything about people stays on your devices. No data brokers, no scraping, no automatic messages: the app drafts, you send.',
  },
  {
    title: 'Learning stays on your device',
    text: 'Samriddhi learns your patterns to order its suggestions, on your device. It cannot change its own code; ideas it notices become a suggestion you may send to the project.',
  },
  {
    title: 'Dated rules with sources',
    text: 'Tax and country rules live in a dated list, checked every two months, each with a link to the official source.',
  },
]

export const GOALS: { kind: GoalKind; label: string; hint: string }[] = [
  { kind: 'emergencyCushion', label: 'Emergency cushion', hint: 'Cash for surprises, so they never become debt.' },
  { kind: 'clearDebt', label: 'Clear debt', hint: 'Pay off what costs you interest.' },
  { kind: 'buyHome', label: 'Buy a home', hint: 'Save a deposit and get ready to borrow.' },
  { kind: 'retireEarly', label: 'Retire early', hint: 'Stop needing to work sooner.' },
  { kind: 'extraIncome', label: 'Extra income', hint: 'Earn more from work, skills or savings.' },
  { kind: 'familySupportAbroad', label: 'Support family abroad', hint: 'Send money home well and cheaply.' },
  { kind: 'learnInvesting', label: 'Learn investing', hint: 'Understand it before putting money in.' },
]

export const MAX_GOALS = 3

export const RATE_BANDS: { band: RateBand; label: string }[] = [
  { band: 'under5', label: 'Under 5% a year' },
  { band: 'from5To10', label: '5% to 10%' },
  { band: 'from10To20', label: '10% to 20%' },
  { band: 'over20', label: 'Over 20% (most credit cards)' },
  { band: 'unknown', label: 'Not sure' },
]

export function screenIndex(id: ScreenId): number {
  return SCREENS.findIndex((s) => s.id === id)
}

export function nextScreen(id: ScreenId): ScreenId {
  return SCREENS[Math.min(screenIndex(id) + 1, SCREENS.length - 1)].id
}

export function prevScreen(id: ScreenId): ScreenId {
  return SCREENS[Math.max(screenIndex(id) - 1, 0)].id
}

/** Where to pick up a wizard that was left part-way (the store already exists). */
export function resumeAt(recoveryPending: boolean): ScreenId {
  return recoveryPending ? 'protect' : 'countries'
}

/** A problem in plain words, or null when the answers can be saved. */
export type Check = string | null

export function checkPin(pin: string, again: string): Check {
  if (!pin && !again) return null
  if (!/^\d+$/.test(pin)) return 'A PIN is digits only.'
  if (pin.length < 4 || pin.length > 12) return 'Use 4 to 12 digits.'
  if (pin !== again) return 'The two PINs are different.'
  return null
}

/** Recovery key as shown: 64 hex digits in groups of 8 for reading aloud or copying by hand. */
export function groupKey(key: string): string {
  return key.replace(/[^0-9a-f]/gi, '').replace(/(.{8})(?=.)/g, '$1 ')
}

export function checkRecoveryText(text: string): Check {
  const hex = text.replace(/[\s-]/g, '')
  if (!hex) return 'Type or paste your recovery key.'
  if (!/^[0-9a-f]+$/i.test(hex)) return 'A recovery key has only the digits 0-9 and letters a-f.'
  if (hex.length !== 64) return `A recovery key is 64 characters; this has ${hex.length}.`
  return null
}

export function checkCountries(list: CountryTie[]): Check {
  if (!list.length) return 'Add at least one country.'
  if (!list.some((c) => c.residence)) return 'Say which country you live in.'
  if (list.filter((c) => c.taxHome).length > 1) return 'Only one country can be where you mainly pay tax.'
  const empty = list.find((c) => !c.residence && !c.taxHome && !c.citizen && !c.holdsMoney)
  if (empty) return 'Tick at least one link for each country, or remove it.'
  return null
}

/** A blank tie for a country just added; the first one added is home. */
export function newTie(code: string, first: boolean): CountryTie {
  return { code, residence: first, taxHome: first, citizen: false, holdsMoney: first, ties: '' }
}

/** Only one tax home: ticking one clears the others. */
export function setTaxHome(list: CountryTie[], code: string, on: boolean): CountryTie[] {
  return list.map((c) => ({ ...c, taxHome: c.code === code ? on : on ? false : c.taxHome }))
}

export interface GoalDraft {
  kind: GoalKind
  target: string
  targetDate: string
}

export function toggleGoal(list: GoalDraft[], kind: GoalKind): GoalDraft[] {
  if (list.some((g) => g.kind === kind)) return list.filter((g) => g.kind !== kind)
  if (list.length >= MAX_GOALS) return list
  return [...list, { kind, target: '', targetDate: '' }]
}

/** Goal drafts to the core's shape, or the first problem. Targets are optional. */
export function buildGoals(list: GoalDraft[], currency: string, today: string): { goals: Goal[] } | { error: string } {
  if (list.length > MAX_GOALS) return { error: `Pick up to ${MAX_GOALS} goals.` }
  const goals: Goal[] = []
  for (const g of list) {
    const label = GOALS.find((x) => x.kind === g.kind)?.label ?? g.kind
    let target: Money | null = null
    if (g.target.trim()) {
      const p = parseAmount(g.target, currency)
      if (!p.ok) return { error: `${label}: ${p.error}` }
      target = p.money
    }
    let targetDate: string | null = null
    if (g.targetDate) {
      if (!/^\d{4}-\d{2}-\d{2}$/.test(g.targetDate)) return { error: `${label}: use a full date.` }
      if (g.targetDate <= today) return { error: `${label}: pick a date in the future.` }
      targetDate = g.targetDate
    }
    goals.push({ kind: g.kind, target, targetDate })
  }
  return { goals }
}

export interface DebtDraft {
  label: string
  balance: string
  band: RateBand
}

export interface MoneyDraft {
  currency: string
  takeHome: string
  costs: string
  cash: string
  /** null = not answered yet; [] = "no debts". */
  debts: DebtDraft[] | null
}

export interface MoneyFacts {
  takeHome: Money | null
  costs: Money | null
  cash: Money | null
  debts: Debt[] | null
}

/** Rough amounts to exact Money, all in one currency. Empty boxes are "skip for now". */
export function buildMoney(d: MoneyDraft): { facts: MoneyFacts } | { error: string } {
  const one = (text: string, what: string): Money | null | string => {
    if (!text.trim()) return null
    const p = parseAmount(text, d.currency)
    return p.ok ? p.money : `${what}: ${p.error}`
  }
  const takeHome = one(d.takeHome, 'Take-home pay')
  const costs = one(d.costs, 'Monthly costs')
  const cash = one(d.cash, 'Cash savings')
  for (const v of [takeHome, costs, cash]) if (typeof v === 'string') return { error: v }
  let debts: Debt[] | null = null
  if (d.debts) {
    debts = []
    for (const [i, x] of d.debts.entries()) {
      const name = x.label.trim()
      if (!name) return { error: `Debt ${i + 1}: give it a short name, like "credit card".` }
      if (name.length > 80) return { error: `Debt ${i + 1}: keep the name under 80 letters.` }
      const b = one(x.balance, name)
      if (b === null) return { error: `${name}: how much is owed? A rough figure is fine.` }
      if (typeof b === 'string') return { error: b }
      debts.push({ label: name, balance: b, band: x.band })
    }
  }
  return { facts: { takeHome: takeHome as Money | null, costs: costs as Money | null, cash: cash as Money | null, debts } }
}

/** Which questions still need an answer, by part. */
export function riskMissing(questions: RiskQuestion[], answers: RiskAnswer[]): { attitude: number; capacity: number } {
  const done = new Set(answers.map((a) => a.questionId))
  const left = questions.filter((q) => !done.has(q.id))
  return {
    attitude: left.filter((q) => q.part === 'attitude').length,
    capacity: left.filter((q) => q.part === 'capacity').length,
  }
}

/** Pick an option, replacing any earlier answer to that question. */
export function answer(list: RiskAnswer[], questionId: string, optionId: string): RiskAnswer[] {
  return [...list.filter((a) => a.questionId !== questionId), { questionId, optionId }]
}

export function checkRisk(questions: RiskQuestion[], answers: RiskAnswer[]): Check {
  const m = riskMissing(questions, answers)
  if (m.attitude + m.capacity === 0) return null
  const parts = [
    m.attitude ? `${m.attitude} about how you feel` : '',
    m.capacity ? `${m.capacity} about what you could afford to lose` : '',
  ].filter(Boolean)
  return `Answer the rest: ${parts.join(' and ')}.`
}

export const CAPACITY_WORDS: Record<string, string> = {
  low: 'Low: a fall would hurt your plans or everyday life.',
  medium: 'Medium: you could take a moderate fall.',
  high: 'High: you could take a large fall without your life changing.',
}

export const ATTITUDE_WORDS = ['', 'Very cautious', 'Cautious', 'Balanced', 'Adventurous', 'Very adventurous']

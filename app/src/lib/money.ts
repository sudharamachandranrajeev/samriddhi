// Money on screen, kept exact: whole minor units (pence, paise, fils) as an
// integer and an ISO 4217 code, the same shape as the core's `Money`. Typed
// text is turned into minor units with string arithmetic, never through a
// floating-point number, and amounts are formatted the same way.

export interface Money {
  minor: number
  currency: string
}

/** Digits after the point where a currency is not 2 (copied from core/src/money.rs). */
const EXPONENTS: Record<string, number> = {
  BHD: 3, BIF: 0, CLP: 0, DJF: 0, GNF: 0, IQD: 3, ISK: 0, JOD: 3, JPY: 0, KMF: 0, KRW: 0,
  KWD: 3, LYD: 3, OMR: 3, PYG: 0, RWF: 0, TND: 3, UGX: 0, UYW: 4, VND: 0, VUV: 0, XAF: 0,
  XOF: 0, XPF: 0,
}

/** The currencies the wizard offers first; any ISO code the core knows also works. */
export const MAIN_CURRENCIES = ['GBP', 'INR', 'AED', 'USD', 'EUR'] as const

const SYMBOLS: Record<string, string> = { GBP: '£', INR: '₹', USD: '$', EUR: '€', JPY: '¥' }

/** The usual currency for a country's money, for a sensible first choice. */
export function currencyFor(country: string | undefined): string {
  return ({ GB: 'GBP', IN: 'INR', AE: 'AED', US: 'USD' } as Record<string, string>)[country ?? ''] ?? 'GBP'
}

export function exponent(currency: string): number {
  return EXPONENTS[currency.toUpperCase()] ?? 2
}

export type Parsed = { ok: true; money: Money } | { ok: false; error: string }

/**
 * Read an amount as people type it: "1,250", "1 250.50", "1,23,456" (Indian
 * grouping), "£900". Too many digits after the point is refused, not rounded.
 * Negative amounts are refused: every wizard amount is a size, not a change.
 */
export function parseAmount(text: string, currency: string): Parsed {
  const t = text.trim().replace(/^[£₹$€¥]|^(AED|GBP|INR|USD|EUR)\s*/i, '').trim()
  if (!t) return { ok: false, error: 'Type an amount, or leave it empty.' }
  if (t.startsWith('-')) return { ok: false, error: 'Amounts cannot be below zero.' }
  const digits = t.replace(/[,\s_]/g, '')
  const m = /^(\d*)(?:\.(\d*))?$/.exec(digits)
  if (!m || (!m[1] && !m[2])) return { ok: false, error: 'Use digits only, like 1,250 or 1250.50.' }
  const exp = exponent(currency)
  const frac = m[2] ?? ''
  if (frac.length > exp) {
    return {
      ok: false,
      error: exp === 0 ? `${currency} has no smaller unit; use whole amounts.` : `Use at most ${exp} digits after the point.`,
    }
  }
  const minor = BigInt((m[1] || '0') + frac.padEnd(exp, '0'))
  if (minor > BigInt(Number.MAX_SAFE_INTEGER)) return { ok: false, error: 'That amount is too large.' }
  return { ok: true, money: { minor: Number(minor), currency: currency.toUpperCase() } }
}

/** Group whole digits: 3s, or Indian lakh/crore grouping for rupees. */
function group(whole: string, indian: boolean): string {
  if (!indian || whole.length <= 3) return whole.replace(/\B(?=(\d{3})+(?!\d))/g, ',')
  const last3 = whole.slice(-3)
  const rest = whole.slice(0, -3).replace(/\B(?=(\d{2})+(?!\d))/g, ',')
  return `${rest},${last3}`
}

export interface FormatOptions {
  /** Drop ".00" when the amount is whole (rough figures). Default true. */
  trim?: boolean
}

/** "£12,400", "₹1,23,456.50", "AED 3,000", "-£250", "1,000 JPY"-style for others. */
export function formatMoney(m: Money, opts: FormatOptions = {}): string {
  if (!Number.isSafeInteger(m.minor)) return '—'
  const exp = exponent(m.currency)
  const neg = m.minor < 0
  const abs = BigInt(Math.abs(m.minor)).toString().padStart(exp + 1, '0')
  const whole = abs.slice(0, abs.length - exp) || '0'
  let frac = exp ? abs.slice(abs.length - exp) : ''
  if ((opts.trim ?? true) && /^0*$/.test(frac)) frac = ''
  const num = group(whole, m.currency === 'INR') + (frac ? `.${frac}` : '')
  const sym = SYMBOLS[m.currency]
  const body = sym ? `${sym}${num}` : m.currency === 'AED' ? `AED ${num}` : `${num} ${m.currency}`
  return neg ? `-${body}` : body
}

/** The plain decimal for an input box: 125050 GBP -> "1250.50"; whole amounts drop ".00". */
/** A model's figure to the nearest whole unit (pound, rupee): its pennies would claim a precision it does not have. */
export function wholeUnits(m: Money): Money {
  const step = 10 ** exponent(m.currency)
  return { ...m, minor: Math.round(m.minor / step) * step }
}

export function toInput(m: Money | null | undefined): string {
  if (!m) return ''
  return formatMoney(m).replace(/^-?[£₹$€¥]|^AED |\s[A-Z]{3}$/g, '').replace(/,/g, '')
}

/**
 * Cash minus debts, the "rough, from your answers" net worth. Same rule as
 * the core's `Profile::rough_net_worth`: nothing until cash and the debts
 * question are both answered, and only when every amount is in one currency.
 */
export function roughNetWorth(cash: Money | null, debts: { balance: Money }[] | null): Money | null {
  if (!cash || !debts) return null
  let total = BigInt(cash.minor)
  for (const d of debts) {
    if (d.balance.currency !== cash.currency) return null
    total -= BigInt(d.balance.minor)
  }
  if (total > BigInt(Number.MAX_SAFE_INTEGER) || total < -BigInt(Number.MAX_SAFE_INTEGER)) return null
  return { minor: Number(total), currency: cash.currency }
}

/** Months the cash covers, to one decimal rounded down as the core does ("1.9"); null without costs. */
export function monthsCovered(cash: Money | null, costs: Money | null): string | null {
  if (!cash || !costs || cash.currency !== costs.currency || costs.minor <= 0) return null
  const tenths = (BigInt(cash.minor) * 10n) / BigInt(costs.minor)
  return `${tenths / 10n}.${tenths % 10n}`
}

/** Add amounts in one currency; null if empty or the currencies differ. */
export function sumMoney(items: Money[]): Money | null {
  if (!items.length) return null
  let total = 0n
  for (const m of items) {
    if (m.currency !== items[0].currency) return null
    total += BigInt(m.minor)
  }
  return total > BigInt(Number.MAX_SAFE_INTEGER) ? null : { minor: Number(total), currency: items[0].currency }
}

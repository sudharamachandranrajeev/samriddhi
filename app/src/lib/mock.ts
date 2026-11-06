// A stand-in for the Rust shell in a plain browser: the preview (`npm run dev`)
// and the screenshot harness. Everything lives in memory and is made up; it
// disappears on reload. It mimics the shell's answers closely enough to try
// every screen, but the real rules live in the Rust core.
//
// `?demo=home` in the address starts with a finished set-up and synthetic
// answers, `?demo=locked` starts locked (PIN 2468), `?demo=recover` starts
// as a device that lost its key, `?demo=update` is a finished set-up that
// updated itself once and sees a newer version.

import type {
  AiSettings, AiSettingsView, BackupKind, Backups, CountryTie, Dismissal, Goal, ImportPreview, Profile, RiskAnswer, RiskProfile, RiskQuestion,
  RollbackPlan, SecurityStatus, Status, Step, UpdateInfo, UpdateMode, UpdateStatus,
} from './api.ts'
import { formatMoney, monthsCovered, roughNetWorth, type Money } from './money.ts'

const o = (id: string, text: string, score: number) => ({ id, text, score })

export const QUESTIONS: RiskQuestion[] = [
  { id: 'a1-fall', part: 'attitude', text: 'Your investments drop by a fifth in one month. What would you most likely do?', why: 'What you would do in a fall tells us more than how you feel on a good day.',
    options: [o('sell-all', 'Sell everything to stop the loss', 1), o('sell-some', 'Sell some of it', 2), o('wait', 'Do nothing and wait', 3), o('keep-adding', 'Keep paying in as before', 4), o('buy-more', 'Put more in while prices are low', 5)] },
  { id: 'a2-aim', part: 'attitude', text: 'Which matters more to you?', why: 'Safety and growth pull against each other; this shows which way you lean.',
    options: [o('never-lose', 'Never losing money, even if it barely grows', 1), o('mostly-safe', 'Mostly safe, with a little growth', 2), o('balance', 'A balance of safety and growth', 3), o('mostly-growth', 'Mostly growth, accepting some falls', 4), o('max-growth', 'As much growth as possible, whatever the swings', 5)] },
  { id: 'a3-choice', part: 'attitude', text: 'You put money away for a year. Which range of results would you pick?', why: 'Bigger possible gains come with bigger possible losses; we want the range you would accept.',
    options: [o('none', 'Lose nothing, gain up to 3%', 1), o('small', 'Lose up to 5%, gain up to 10%', 2), o('medium', 'Lose up to 10%, gain up to 20%', 3), o('large', 'Lose up to 20%, gain up to 35%', 4), o('very-large', 'Lose up to 35%, gain up to 60%', 5)] },
  { id: 'a4-feel', part: 'attitude', text: 'How do you feel when you think about investing?', why: 'Choices that keep you awake at night are not good choices, however they turn out.',
    options: [o('anxious', 'Anxious; I would rather not', 1), o('uneasy', 'Uneasy, but I know I should', 2), o('fine', 'Fine, as long as I understand it', 3), o('keen', 'Keen', 4), o('thrilled', 'Excited by the chance of big gains', 5)] },
  { id: 'a5-below', part: 'attitude', text: 'How long could you watch an investment stay below what you paid before it worried you a lot?', why: 'Most investments spend time below their price; patience is part of the risk you take.',
    options: [o('days', 'A few days', 1), o('months', 'A few months', 2), o('year', 'About a year', 3), o('years', 'A few years', 4), o('no-worry', 'It would not worry me', 5)] },
  { id: 'a6-friends', part: 'attitude', text: 'How would people who know you describe you with money?', why: 'Others often see our habits more clearly than we do.',
    options: [o('very-careful', 'Very careful', 1), o('careful', 'Careful', 2), o('middle', 'Somewhere in the middle', 3), o('bold', 'Willing to take a chance', 4), o('very-bold', 'A real risk-taker', 5)] },
  { id: 'c1-when', part: 'capacity', text: 'When might you need the money you would invest?', why: 'Money needed soon has no time to recover from a fall.',
    options: [o('under-2', 'Within 2 years', 0), o('2-to-5', 'In 2 to 5 years', 1), o('over-5', 'Not for more than 5 years', 2)] },
  { id: 'c2-hit', part: 'capacity', text: 'If that money fell by a third, how would your everyday life change?', why: 'A loss you can absorb is very different from one that changes how you live.',
    options: [o('hard', 'A lot: I would struggle with bills or plans', 0), o('tighter', 'Things would be tighter for a while', 1), o('no-change', 'Not at all', 2)] },
  { id: 'c3-cushion', part: 'capacity', text: 'If your income stopped, how long could you live on your cash?', why: 'A cash cushion means you never have to sell investments at a bad time.',
    options: [o('under-3', 'Less than 3 months', 0), o('3-to-6', '3 to 6 months', 1), o('over-6', 'More than 6 months', 2)] },
  { id: 'c4-income', part: 'capacity', text: 'How steady is your income?', why: 'Steady income can refill savings after a fall; uncertain income cannot.',
    options: [o('unsteady', 'Uncertain, or none at the moment', 0), o('mostly', 'Mostly steady', 1), o('steady', 'Very steady, or from several sources', 2)] },
]

const LABELS = ['', 'Very cautious', 'Cautious', 'Balanced', 'Adventurous', 'Very adventurous']

function score(answers: RiskAnswer[]): RiskProfile | null {
  const picked = new Map(answers.map((a) => [a.questionId, a.optionId]))
  let att = 0
  let cap = 0
  for (const q of QUESTIONS) {
    const opt = q.options.find((x) => x.id === picked.get(q.id))
    if (!opt) return null
    if (q.part === 'attitude') att += opt.score
    else cap += opt.score
  }
  const attitude = Math.min(5, Math.floor((att - 6) / 5) + 1)
  let capacity: RiskProfile['capacity'] = cap <= 2 ? 'low' : cap <= 5 ? 'medium' : 'high'
  if (picked.get('c1-when') === 'under-2') capacity = 'low'
  const combined = Math.min(attitude, { low: 2, medium: 3, high: 5 }[capacity])
  const words = { low: 'a fall would hurt your plans or everyday life', medium: 'you could take a moderate fall', high: 'you could take a large fall without your life changing' }[capacity]
  let summary = `You said you are comfortable with ${LABELS[attitude].toLowerCase()} risk (${attitude} of 5), and ${words}.`
  summary += combined < attitude
    ? ` Because of what you could afford to lose, we keep suggestions at ${LABELS[combined].toLowerCase()} (${combined} of 5).`
    : ` We keep suggestions at ${LABELS[combined].toLowerCase()} (${combined} of 5).`
  return { attitude, capacity, combined, label: LABELS[combined], summary }
}

const RECOVERY = '3f9c2a7e1b5d48c06a2e9f71d3b8c45e0a6f2d9b7c1e3a5f8d2b4c6e9a1f3d7b'
const GBP = (minor: number): Money => ({ minor, currency: 'GBP' })

interface Db {
  exists: boolean
  open: boolean
  locked: boolean
  settings: Record<string, unknown>
  facts: Record<string, { value: unknown; source: string; at: string }>
  countries: CountryTie[]
  goals: Goal[]
  answers: RiskAnswer[]
  dismissed: Dismissal[]
  /** Step ids accepted (the core hides them for 14 days; here, until reload). */
  accepted: string[]
  backups: { name: string; kind: BackupKind; bytes: number }[]
  /** A newer version the preview pretends is out (the ?demo=update page). */
  update: UpdateInfo | null
  pending: ImportPreview | null
  picked: boolean
  wrong: number
}

const fresh = (): Db => ({
  exists: false, open: false, locked: false, settings: {}, facts: {}, countries: [], goals: [], answers: [], dismissed: [], accepted: [],
  backups: [], pending: null, picked: false, wrong: 0, update: null,
})

const NOW = '2026-11-06T09:00:00Z'
const TODAY = NOW.slice(0, 10)

function demo(db: Db) {
  db.exists = true
  db.open = true
  db.settings = { 'terms.accepted': true, 'setup.done': true, 'recovery.saved': true }
  const fact = (k: string, v: unknown) => (db.facts[k] = { value: v, source: 'wizard', at: NOW })
  fact('money.takeHomeMonthly', GBP(310000))
  fact('money.costsMonthly', GBP(215000))
  fact('money.cashSavings', GBP(420000))
  fact('money.debts', [{ label: 'Credit card', balance: GBP(180000), band: 'over20' }, { label: 'Car loan', balance: GBP(640000), band: 'from5To10' }])
  db.countries = [
    { code: 'GB', residence: true, taxHome: true, citizen: false, holdsMoney: true, ties: '' },
    { code: 'IN', residence: false, taxHome: false, citizen: true, holdsMoney: true, ties: '' },
    { code: 'AE', residence: false, taxHome: false, citizen: false, holdsMoney: true, ties: '' },
  ]
  db.goals = [{ kind: 'emergencyCushion', target: GBP(1300000), targetDate: '2027-12-31' }, { kind: 'clearDebt', target: null, targetDate: null }]
  db.answers = QUESTIONS.map((q) => ({ questionId: q.id, optionId: q.options[q.part === 'attitude' ? 2 : 1].id }))
  db.backups = [
    { name: 'samriddhi-2026-11-06.db', kind: 'daily', bytes: 98304 },
    { name: 'samriddhi-2026-11-05.db', kind: 'daily', bytes: 98304 },
    { name: 'manual-2026-11-01-101500.db', kind: 'manual', bytes: 94208 },
  ]
}

const db = fresh()
const mode = typeof location !== 'undefined' ? new URLSearchParams(location.search).get('demo') : null
if (mode === 'home' || mode === 'locked') demo(db)
if (mode === 'locked') {
  db.locked = true
  db.settings['lock.pin'] = '2468'
}
if (mode === 'update') {
  // A copy that updated itself once (0.1.0 to 0.1.1) and sees 0.1.2 is out.
  demo(db)
  db.settings['update.previous'] = '0.1.0'
  db.backups.splice(1, 0, { name: 'pre-update-2026-11-05-181500-0.1.0.db', kind: 'preUpdate', bytes: 98304 })
  db.update = {
    version: '0.1.2',
    notes: '### Fixed\n- The home screen keeps your theme after a restore.\n\n### Added\n- Clearer dates on backups.',
    date: '2026-11-20T09:00:00Z',
    size: 9_437_184,
    canInstall: true,
  }
}
if (mode === 'recover') {
  demo(db)
  db.open = false
}

function profile(): Profile {
  const f = (k: string) => (db.facts[k]?.value ?? null) as never
  return {
    takeHome: f('money.takeHomeMonthly'),
    costs: f('money.costsMonthly'),
    cash: f('money.cashSavings'),
    debts: f('money.debts'),
    pensionMatch: f('pension.employerMatch'),
    isaUsed: f('isa.used'),
    countries: db.countries,
    goals: db.goals,
    risk: score(db.answers),
  }
}

const SRC = 'https://www.moneyhelper.org.uk/en/savings/how-to-save/emergency-savings-how-much-is-enough'

function steps(): Step[] {
  const p = profile()
  const out: Step[] = []
  const step = (id: string, title: string, why: string, extra: Partial<Step> = {}): Step =>
    ({ id, origin: 'steps', kind: 'action', title, why, risk: 1, effort: 'minutes', source: SRC, asOf: '2026-10-30', ask: null, basis: null, disclosure: null, ...extra })
  const months = monthsCovered(p.cash, p.costs)
  if (p.cash && p.costs && months !== null && Number(months) < 3) {
    const short = { minor: p.costs.minor * 3 - p.cash.minor, currency: p.cash.currency }
    out.push(step('cushion', 'Build a 3-month safety cushion',
      `Your cash covers about ${months} months of your costs. Keeping at least 3 months in an easy-access account means a surprise bill or a gap in pay does not turn into debt. You need about ${formatMoney(short)} more.`,
      { effort: 'ongoing' }))
  }
  const costly = (p.debts ?? []).filter((d) => d.band === 'from10To20' || d.band === 'over20')
  if (costly.length) {
    out.push(step('high-interest-debt', 'Pay down your most expensive debt',
      `${costly.map((d) => d.label).join(', ')} cost 10% a year or more. Paying it off earns that rate back for certain, which savings rarely match. Keep paying the minimum on everything, then put anything extra on the highest rate first.`,
      { effort: 'ongoing', source: 'https://www.moneyhelper.org.uk/en/money-troubles/dealing-with-debt' }))
  }
  if (p.countries.some((c) => c.code === 'GB' && c.taxHome && c.residence)) {
    out.push(step('isa-allowance-2026', 'Use your ISA allowance before 5 April 2027',
      'In the 2026-27 tax year you can put up to £20,000 into ISAs. Interest and growth inside an ISA are free of UK tax, and allowance you do not use is lost on 5 April 2027.',
      { effort: 'hours', risk: 2, source: 'https://www.gov.uk/individual-savings-accounts' }))
    if (!p.pensionMatch || p.pensionMatch === 'unsure') {
      out.push(step('ask:pension.employerMatch', 'Does your employer add more to your pension when you pay in more?',
        'Some employers pay in more than the legal minimum when you do. If yours does, not using it is like turning down part of your pay.',
        { ask: { fact: 'pension.employerMatch' }, source: 'https://www.gov.uk/workplace-pensions' }))
    }
  }
  if (!p.costs) out.push(step('ask:money.costsMonthly', 'Roughly how much do you spend in a month?', 'Your costs set the size of the safety cushion you need.', { ask: { fact: 'money.costsMonthly' } }))
  const hidden = (id: string) => db.dismissed.some((d) => d.stepId === id)
  return out.filter((s) => !hidden(s.id))
}

const DISCLOSURE = 'This is a model using illustrative assumptions, not financial advice or a guarantee. Past and simulated results do not predict future returns.'

/** A few made-up agent steps, shaped like the core's, once the risk questions are answered. */
function agentSteps(): Step[] {
  const p = profile()
  if (!p.risk) return []
  const agent = (origin: string, id: string, kind: Step['kind'], title: string, why: string, basis: string, extra: Partial<Step> = {}): Step =>
    ({ id, origin, kind, title, why, risk: 1, effort: 'minutes', source: 'Generic long-run illustrative figures (synthetic)', asOf: '2026-11-05', ask: null, basis, disclosure: null, ...extra })
  return [
    agent('rebalancing', 'rebalance:trades', 'action', 'Rebalance to target',
      'Sell Equities 9,000 GBP; Buy Bonds 9,000 GBP. Putting new money into the classes that are under target first can do most of this without selling (see Insights). Samriddhi never trades: this is for you to consider.',
      'Exact arithmetic on the holdings you entered; a threshold rule (a Vanguard-style 3% band), not a market view.',
      { risk: 2, effort: 'hours', disclosure: DISCLOSURE, asOf: TODAY }),
    agent('risk-profile', 'risk:band', 'info', `${p.risk.label} risk profile`,
      `Your answers put you at level ${p.risk.combined} of 5.`, 'Rule-based from your answers; as reliable as those answers are.',
      { source: 'Your answers to the risk questions', asOf: TODAY }),
    agent('portfolio', 'portfolio:target', 'info', 'Target allocation', 'Equities 45%, Bonds 40%, Property / REITs 5%, Gold / commodities 5%, Cash 5%.',
      'Mean-variance optimiser on illustrative long-run assumptions, not market data. Treat it as a starting point.', { disclosure: DISCLOSURE }),
  ].filter((s) => !db.dismissed.some((d) => d.stepId === s.id) && !db.accepted.includes(s.id))
}

const AI_OFF: AiSettings = { enabled: false, provider: 'local', endpoint: '', model: '' }

const summary = () => ({
  schema: 1,
  counts: { sam_profile: Object.keys(db.facts).length, sam_countries: db.countries.length, sam_goals: db.goals.length, sam_risk_answers: db.answers.length, sam_steps_dismissed: db.dismissed.length },
})

function status(): Status {
  return {
    phase: !db.exists ? 'needsSetup' : !db.open ? 'needsRecovery' : db.locked ? 'locked' : 'open',
    problem: db.exists && !db.open ? 'The key kept by this device is missing.' : null,
    recoveryPending: db.open && !db.settings['recovery.saved'],
    keyStorage: 'keyring',
    hasPin: Boolean(db.settings['lock.pin']),
    biometric: Boolean(db.settings['lock.biometric']),
    waitSecs: 0,
    platform: 'linux',
    notice: null,
  }
}

function needOpen() {
  if (!db.open) throw 'Your data is not open yet: set up Samriddhi or enter your recovery key.'
  if (db.locked) throw 'Samriddhi is locked.'
}

// Arguments arrive as loose JSON, as they would over the bridge.
type Args = Record<string, any>

const handlers: Record<string, (a: Args) => unknown> = {
  status,
  create_store: () => {
    if (db.exists) throw 'Samriddhi is already set up on this device.'
    db.exists = db.open = true
    db.backups.push({ name: `samriddhi-${TODAY}.db`, kind: 'daily', bytes: 65536 })
    return RECOVERY
  },
  recovery_key: () => {
    needOpen()
    if (db.settings['recovery.saved']) throw 'The recovery key was shown once and you confirmed you saved it.'
    return RECOVERY
  },
  recovery_saved: () => { needOpen(); db.settings['recovery.saved'] = true; return null },
  unlock_with_recovery: ({ recovery }) => {
    if (recovery.replace(/[\s-]/g, '').toLowerCase() !== RECOVERY) throw 'That recovery key does not open this data.'
    db.open = true
    db.locked = false
    return null
  },
  set_pin: ({ pin }) => {
    needOpen()
    if (!/^\d{4,}$/.test(pin)) throw 'a PIN needs at least 4 digits, and only digits'
    db.settings['lock.pin'] = pin
    return null
  },
  clear_pin: () => { needOpen(); delete db.settings['lock.pin']; return null },
  unlock_pin: ({ pin }) => {
    if (!db.settings['lock.pin']) throw 'No PIN is set.'
    if (pin === db.settings['lock.pin']) { db.locked = false; db.wrong = 0; return true }
    db.wrong++
    return false
  },
  unlock_biometric: () => { throw 'Fingerprint and face unlock are for phones; use a PIN on this computer.' },
  set_biometric: ({ on }) => {
    if (on) throw 'Fingerprint and face unlock are for phones; use a PIN on this computer.'
    db.settings['lock.biometric'] = false
    return null
  },
  lock: () => {
    db.locked = Boolean(db.settings['lock.pin'] || db.settings['lock.biometric'])
    return db.locked
  },
  get_setting: ({ key }) => { needOpen(); return db.settings[key] ?? null },
  put_setting: ({ key, value }) => { needOpen(); db.settings[key] = value; return null },
  backups: (): Backups => {
    needOpen()
    const folder = (db.settings['backup.folder'] as string | undefined) ?? null
    return { folder: '/home/user/.local/share/dev.brahma.samriddhi/backups', files: db.backups, userFolder: folder, userFiles: folder ? [`samriddhi-${TODAY}.db`] : [] }
  },
  set_backup_folder: ({ folder }) => { needOpen(); db.settings['backup.folder'] = folder; return null },
  backup_now: () => {
    needOpen()
    const name = `manual-${TODAY}-090000.db`
    db.backups.unshift({ name, kind: 'manual', bytes: 98304 })
    return name
  },
  save_backup_to: () => { needOpen(); return null },
  pick_backup: () => { db.picked = true; return 'samriddhi-backup.db' },
  inspect_backup: () => { needOpen(); return summary() },
  restore_backup: () => {
    needOpen()
    const name = `before-restore-${TODAY}-090100.db`
    db.backups.unshift({ name, kind: 'beforeRestore', bytes: 98304 })
    return name
  },
  profile_get: () => {
    needOpen()
    const p = profile()
    return {
      profile: p,
      facts: Object.entries(db.facts).map(([key, f]) => ({ key, value: f.value, source: f.source, askedAt: f.at, why: '' })),
      roughNetWorth: roughNetWorth(p.cash, p.debts),
    }
  },
  set_fact: ({ key, value, source }) => { needOpen(); db.facts[key] = { value, source, at: NOW }; return null },
  clear_fact: ({ key }) => { needOpen(); delete db.facts[key]; return null },
  set_countries: ({ countries }) => {
    needOpen()
    if (countries.filter((c: CountryTie) => c.taxHome).length > 1) throw 'only one country can be your tax home'
    db.countries = countries
    return null
  },
  set_goals: ({ goals }) => {
    needOpen()
    if (goals.length > 3) throw 'pick up to 3 goals'
    db.goals = goals
    return null
  },
  risk_questions: () => QUESTIONS,
  save_risk_answers: ({ answers }) => { needOpen(); db.answers = answers; return score(answers) },
  risk_result: () => { needOpen(); return score(db.answers) },
  steps_list: () => { needOpen(); return { steps: steps(), reviewDue: false } },
  step_later: ({ id }) => { needOpen(); db.dismissed.push({ stepId: id, kind: 'later', reason: null, on: TODAY }); return null },
  step_not_for_me: ({ id, reason }) => {
    needOpen()
    db.dismissed.push({ stepId: id, kind: 'notForMe', reason: reason.trim() || null, on: TODAY })
    return null
  },
  step_restore: ({ id }) => { needOpen(); db.dismissed = db.dismissed.filter((d) => d.stepId !== id); return null },
  steps_dismissed: () => { needOpen(); return db.dismissed },
  feed_list: () => {
    needOpen()
    const rank = (s: Step) => (s.ask ? 3 : s.kind === 'action' ? (s.origin === 'steps' ? 0 : 1) : s.kind === 'note' ? 2 : 4)
    const all = [...steps(), ...agentSteps()].filter((s) => !db.accepted.includes(s.id))
    return { steps: all.sort((a, b) => rank(a) - rank(b)), reviewDue: false, errors: [] }
  },
  step_accept: ({ id }) => {
    needOpen()
    const s = [...steps(), ...agentSteps()].find((x) => x.id === id)
    if (!s) throw 'that step is not in your list now'
    if (s.kind !== 'action' || s.ask) throw 'only a suggested action can be accepted; this one can be put off'
    db.accepted.push(id)
    return null
  },
  ai_settings_get: (): AiSettingsView => {
    needOpen()
    const settings = (db.settings['ai.settings'] as AiSettings | undefined) ?? AI_OFF
    return { settings, keySet: Boolean(db.settings['ai.keySet']), keyProblem: null, problem: settings.model ? null : 'Enter a model name.', localSuggestion: 'http://localhost:11434/v1', anthropicSuggestion: 'https://api.anthropic.com', anthropicModel: 'claude-sonnet-5-5', canSend: true }
  },
  ai_settings_set: ({ settings }) => { needOpen(); db.settings['ai.settings'] = settings; return null },
  ai_key_set: () => { needOpen(); db.settings['ai.keySet'] = true; return null },
  ai_key_clear: () => { needOpen(); db.settings['ai.keySet'] = false; return null },
  ai_preview: () => {
    needOpen()
    const settings = (db.settings['ai.settings'] as AiSettings | undefined) ?? AI_OFF
    if (!settings.enabled) throw 'AI narration is off. Turn it on in Settings first.'
    const text = JSON.stringify({ risk_profile: 'Balanced', total_invested_approx: 100000, goals: [{ name: 'Goal 1', horizon_years: 20 }], note: 'Figures are rounded model outputs using illustrative assumptions.' }, null, 2)
    return { system: 'You explain an investment plan in plain English to a non-expert.', text, destination: 'localhost', provider: settings.provider }
  },
  ai_send: () => { throw 'The browser preview never sends anything to an AI model.' },
  security_status: (): SecurityStatus => {
    needOpen()
    const hasPin = Boolean(db.settings['lock.pin'])
    return {
      rows: [
        { id: 'lock', label: 'App lock', level: hasPin ? 'good' : 'info', detail: hasPin ? 'On, with a PIN.' : 'Off. Set a PIN to hide your data when you step away.' },
        { id: 'biometric', label: 'Fingerprint or face', level: 'na', detail: 'For phones; use a PIN on this computer.' },
        { id: 'captureBlock', label: 'Block screenshots and screen recording', level: 'na', detail: 'For phones.' },
        { id: 'dbEncrypted', label: 'Data encrypted', level: 'good', detail: 'Yes. The key is kept in the system keyring.' },
        { id: 'lastBackup', label: 'Last backup', level: 'good', detail: 'less than an hour ago' },
      ],
    }
  },
  set_block_capture: () => { throw 'Blocking screenshots is for phones.' },
  export_json: () => { needOpen(); return null },
  export_csv: ({ folder }) => { needOpen(); return `${folder}/samriddhi-export-${TODAY}-090000` },
  import_pick: (): ImportPreview => {
    needOpen()
    db.pending = {
      exportedAt: '2026-10-31T18:20:00Z',
      schema: 1,
      tables: [
        { table: 'sam_profile', current: Object.keys(db.facts).length, incoming: 5 },
        { table: 'sam_countries', current: db.countries.length, incoming: 2 },
        { table: 'sam_goals', current: db.goals.length, incoming: 3 },
        { table: 'sam_risk_answers', current: db.answers.length, incoming: 10 },
        { table: 'sam_steps_dismissed', current: db.dismissed.length, incoming: 1 },
      ],
      kept: ['sam_settings', 'sam_audit'],
    }
    return db.pending
  },
  import_apply: () => {
    needOpen()
    if (!db.pending) throw 'Choose an export file first.'
    db.pending = null
    return `before-import-${TODAY}-090200.db`
  },
  import_cancel: () => { db.pending = null; return null },
  update_status: (): UpdateStatus => {
    needOpen()
    return {
      kind: 'appImage',
      canInstall: true,
      current: db.update ? '0.1.1' : '0.1.0',
      previous: (db.settings['update.previous'] as string | undefined) ?? null,
      mode: (db.settings['updates.mode'] as UpdateMode | undefined) ?? 'ask',
    }
  },
  set_update_mode: ({ mode }) => { needOpen(); db.settings['updates.mode'] = mode; return null },
  check_update: () => db.update,
  install_update: () => { throw 'The browser preview cannot install updates.' },
  rollback_plan: (): RollbackPlan | null => {
    needOpen()
    const version = db.settings['update.previous'] as string | undefined
    if (!version) return null
    const backup = db.backups.find((b) => b.kind === 'preUpdate' && b.name.endsWith(`-${version}.db`))?.name ?? null
    return { version, backup, needsRestore: false }
  },
  rollback: () => { throw 'The browser preview cannot install other versions.' },
  open_page: () => { throw 'The browser preview cannot open web pages.' },
}

export async function invoke(cmd: string, args: Args = {}): Promise<unknown> {
  const h = handlers[cmd]
  if (!h) throw `Unknown command ${cmd}`
  // A short pause, like a real round trip, so loading states are seen.
  await new Promise((r) => setTimeout(r, 30))
  return structuredClone(h(args))
}

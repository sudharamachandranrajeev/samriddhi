// The Rust shell's commands, with the exact names and shapes the shell uses
// (serde camelCase). Inside the app they go to Tauri; in a plain browser (the
// preview and the screenshot harness) a small in-memory stand-in with
// made-up data answers instead, loaded only there.

import type { Money } from './money.ts'

export type Phase = 'needsSetup' | 'needsRecovery' | 'locked' | 'open'
export type KeyStorage = 'keyring' | 'file' | 'androidKeystore'

export interface Status {
  phase: Phase
  problem: string | null
  recoveryPending: boolean
  keyStorage?: KeyStorage | null
  hasPin: boolean
  biometric: boolean
  waitSecs: number
  platform: string
  notice: string | null
}

export type Source = 'wizard' | 'asked'
export type RateBand = 'under5' | 'from5To10' | 'from10To20' | 'over20' | 'unknown'
export type PensionMatch = 'yes' | 'no' | 'notEmployed' | 'unsure'
export type GoalKind =
  | 'emergencyCushion'
  | 'clearDebt'
  | 'buyHome'
  | 'retireEarly'
  | 'extraIncome'
  | 'familySupportAbroad'
  | 'learnInvesting'

export interface Debt {
  label: string
  balance: Money
  band: RateBand
}

export interface CountryTie {
  code: string
  residence: boolean
  taxHome: boolean
  citizen: boolean
  holdsMoney: boolean
  ties: string
}

export interface Goal {
  kind: GoalKind
  target: Money | null
  /** YYYY-MM-DD */
  targetDate: string | null
}

export type Capacity = 'low' | 'medium' | 'high'

export interface RiskProfile {
  attitude: number
  capacity: Capacity
  combined: number
  label: string
  summary: string
}

export interface Profile {
  takeHome: Money | null
  costs: Money | null
  cash: Money | null
  debts: Debt[] | null
  pensionMatch: PensionMatch | null
  isaUsed: { taxYear: number; amount: Money } | null
  countries: CountryTie[]
  goals: Goal[]
  risk: RiskProfile | null
}

export interface Fact {
  key: string
  value: unknown
  source: Source | 'derived'
  askedAt: string
  why: string
}

export interface ProfileView {
  profile: Profile
  facts: Fact[]
  roughNetWorth: Money | null
}

export interface RiskOption {
  id: string
  text: string
  score: number
}

export interface RiskQuestion {
  id: string
  part: 'attitude' | 'capacity'
  text: string
  why: string
  options: RiskOption[]
}

export interface RiskAnswer {
  questionId: string
  optionId: string
}

export type Effort = 'minutes' | 'hours' | 'ongoing'
/** Only an `action` without a question can be accepted. */
export type StepKind = 'action' | 'info' | 'note'

export interface Step {
  id: string
  /** 'steps' for the rules, or the agent that suggested it ('rebalancing', 'behaviour'...). */
  origin: string
  kind: StepKind
  title: string
  why: string
  risk: number
  effort: Effort
  source: string
  asOf: string
  ask: { fact: string } | null
  /** How far to trust it; every agent step has one. */
  basis: string | null
  /** The model disclosure, on steps built on the plan or forecast. Show it with the step. */
  disclosure: string | null
}

export interface StepsView {
  steps: Step[]
  reviewDue: boolean
}

/** The rules' steps and the agents' steps in one order. */
export interface FeedView {
  steps: Step[]
  reviewDue: boolean
  /** Agents that could not run this time. */
  errors: string[]
}

export type AiProvider = 'local' | 'cloud'

/** AI narration settings. The key is never here: it lives in the OS keyring. */
export interface AiSettings {
  enabled: boolean
  provider: AiProvider
  endpoint: string
  model: string
}

export interface AiSettingsView {
  settings: AiSettings
  /** Only whether a key is kept, never the key. */
  keySet: boolean
  keyProblem: string | null
  /** Why the settings cannot be used yet. */
  problem: string | null
  localSuggestion: string
  /** False on phones for now: preview only. */
  canSend: boolean
}

/** Exactly what `ai_send` would send, for the user to read first. */
export interface AiPreview {
  system: string
  text: string
  destination: string
  provider: AiProvider
}

export type BackupKind = 'daily' | 'manual' | 'preUpdate' | 'beforeRestore' | 'beforeImport'

export interface Backup {
  name: string
  kind: BackupKind
  bytes: number
}

export interface Backups {
  folder: string
  files: Backup[]
  userFolder: string | null
  userFiles: string[]
}

/** What a backup holds: the data version and rows per table. */
export interface Summary {
  schema: number
  counts: Record<string, number>
}

export interface TableCount {
  table: string
  current: number
  incoming: number
}

export interface ImportPreview {
  exportedAt: string
  schema: number
  tables: TableCount[]
  kept: string[]
}

export type InstallKind = 'installer' | 'appImage' | 'pacman' | 'portable' | 'android' | 'other'
export type UpdateMode = 'ask' | 'off'

export interface UpdateStatus {
  kind: InstallKind
  canInstall: boolean
  current: string
  /** The version before the last update, which "go back" returns to. */
  previous: string | null
  mode: UpdateMode
}

export interface UpdateInfo {
  version: string
  notes: string | null
  date: string | null
  /** Download size in bytes, when the update list gives it. */
  size: number | null
  canInstall: boolean
}

export interface RollbackPlan {
  version: string
  backup: string | null
  needsRestore: boolean
}

/** Pages the app may open in the browser: its download page or a new GitHub issue. */
export type Page = 'releases' | 'bug' | 'idea'

/** Every command the screens use, with its arguments and answer. */
export interface Commands {
  status: [Record<string, never>, Status]
  create_store: [Record<string, never>, string]
  recovery_key: [Record<string, never>, string]
  recovery_saved: [Record<string, never>, null]
  unlock_with_recovery: [{ recovery: string }, null]
  set_pin: [{ pin: string }, null]
  clear_pin: [Record<string, never>, null]
  unlock_pin: [{ pin: string }, boolean]
  unlock_biometric: [Record<string, never>, null]
  set_biometric: [{ on: boolean }, null]
  lock: [Record<string, never>, boolean]
  get_setting: [{ key: string }, unknown]
  put_setting: [{ key: string; value: unknown }, null]
  backups: [Record<string, never>, Backups]
  set_backup_folder: [{ folder: string | null }, null]
  backup_now: [Record<string, never>, string]
  save_backup_to: [{ target: string }, null]
  pick_backup: [Record<string, never>, string | null]
  inspect_backup: [{ name: string | null; recovery: string | null }, Summary]
  restore_backup: [{ name: string | null; recovery: string | null }, string]
  update_status: [Record<string, never>, UpdateStatus]
  set_update_mode: [{ mode: UpdateMode }, null]
  check_update: [Record<string, never>, UpdateInfo | null]
  install_update: [{ version: string }, null]
  rollback_plan: [Record<string, never>, RollbackPlan | null]
  rollback: [{ restore: boolean }, null]
  open_page: [{ page: Page; fields: Record<string, string> }, null]
  profile_get: [Record<string, never>, ProfileView]
  set_fact: [{ key: string; value: unknown; source: Source }, null]
  clear_fact: [{ key: string }, null]
  set_countries: [{ countries: CountryTie[]; source: Source }, null]
  set_goals: [{ goals: Goal[]; source: Source }, null]
  risk_questions: [Record<string, never>, RiskQuestion[]]
  save_risk_answers: [{ answers: RiskAnswer[] }, RiskProfile | null]
  risk_result: [Record<string, never>, RiskProfile | null]
  steps_list: [Record<string, never>, StepsView]
  step_later: [{ id: string }, null]
  step_not_for_me: [{ id: string; reason: string }, null]
  step_restore: [{ id: string }, null]
  steps_dismissed: [Record<string, never>, Dismissal[]]
  feed_list: [Record<string, never>, FeedView]
  /** Records intent in the step log only; nothing acts on it. */
  step_accept: [{ id: string }, null]
  ai_settings_get: [Record<string, never>, AiSettingsView]
  ai_settings_set: [{ settings: AiSettings }, null]
  ai_key_set: [{ key: string }, null]
  ai_key_clear: [Record<string, never>, null]
  ai_preview: [Record<string, never>, AiPreview]
  /** `text` must be exactly the last preview's text. The answer is plain text to show, never markup. */
  ai_send: [{ text: string }, string]
  export_json: [{ target: string }, null]
  export_csv: [{ folder: string }, string]
  import_pick: [Record<string, never>, ImportPreview | null]
  import_apply: [Record<string, never>, string]
  import_cancel: [Record<string, never>, null]
}

export interface Dismissal {
  stepId: string
  kind: 'later' | 'notForMe'
  reason: string | null
  /** YYYY-MM-DD */
  on: string
}

export type Command = keyof Commands
type Args<C extends Command> = Commands[C][0]
type Answer<C extends Command> = Commands[C][1]

export const inApp = typeof globalThis !== 'undefined' && '__TAURI_INTERNALS__' in globalThis

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>
let backend: Promise<Invoke> | null = null

function load(): Promise<Invoke> {
  backend ??= inApp
    ? import('@tauri-apps/api/core').then((m) => m.invoke as Invoke)
    : import('./mock.ts').then((m) => m.invoke)
  return backend
}

/** Call a command. Errors from Rust arrive as plain-language strings. */
export async function call<C extends Command>(
  cmd: C,
  ...args: Args<C> extends Record<string, never> ? [] : [Args<C>]
): Promise<Answer<C>> {
  const invoke = await load()
  try {
    return (await invoke(cmd, args[0] as Record<string, unknown> | undefined)) as Answer<C>
  } catch (e) {
    throw new Error(typeof e === 'string' ? e : e instanceof Error ? e.message : String(e))
  }
}

/** Save and open dialogs. In the browser preview they return a made-up path. */
export async function chooseSave(title: string, defaultPath: string, ext: string): Promise<string | null> {
  if (!inApp) return `/home/user/Documents/${defaultPath}`
  const { save } = await import('@tauri-apps/plugin-dialog')
  return save({ title, defaultPath, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] })
}

export async function chooseFolder(title: string): Promise<string | null> {
  if (!inApp) return '/home/user/Backups'
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ title, directory: true, multiple: false })
  return typeof picked === 'string' ? picked : null
}

/** Download progress of an update: bytes so far and the total, if known. */
export async function onUpdateProgress(f: (done: number, total: number | null) => void): Promise<() => void> {
  if (!inApp) return () => {}
  const { listen } = await import('@tauri-apps/api/event')
  return listen<[number, number | null]>('update-progress', (e) => f(e.payload[0], e.payload[1]))
}

// The one place that decides how the whole app looks. Setting `ui.theme`, `ui.mode` or
// `ui.motion` restyles every screen at once through attributes on <html>.

import { tokens, defaultTheme } from './generated/tokens.ts'
import { cssBlock, themeSelector, themeVars, type Mode, type ThemeDef } from './compile.ts'

export type ModePref = 'system' | 'light' | 'dark'
export type MotionPref = 'system' | 'calm' | 'full'

export interface UiPrefs {
  theme: string
  mode: ModePref
  motion: MotionPref
  custom: Record<string, ThemeDef>
}

export interface PrefsStore {
  load(): UiPrefs | null
  save(prefs: UiPrefs): void
}

const KEY = 'samriddhi.ui'

// Browser storage is a stand-in until the app hands over its own encrypted settings store.
export const browserStore: PrefsStore = {
  load() {
    try {
      const raw = localStorage.getItem(KEY)
      return raw ? (JSON.parse(raw) as UiPrefs) : null
    } catch {
      return null
    }
  },
  save(prefs) {
    try {
      localStorage.setItem(KEY, JSON.stringify(prefs))
    } catch {
      // Private window or blocked storage: the choice lasts for this session only.
    }
  },
}

const darkQuery = () => matchMedia('(prefers-color-scheme: dark)')
const calmQuery = () => matchMedia('(prefers-reduced-motion: reduce)')

class Ui {
  theme = $state(defaultTheme)
  mode = $state<ModePref>('system')
  motion = $state<MotionPref>('system')
  custom = $state<Record<string, ThemeDef>>({})
  systemDark = $state(false)
  systemCalm = $state(false)
  #store: PrefsStore = browserStore

  /** Every theme the user can pick: built-ins first, then their own. */
  get themes(): [string, ThemeDef][] {
    return [...Object.entries(tokens.themes), ...Object.entries(this.custom)]
  }

  get current(): ThemeDef {
    return this.custom[this.theme] ?? tokens.themes[this.theme] ?? tokens.themes[defaultTheme]
  }

  get resolvedMode(): Mode {
    const pref = this.mode === 'system' ? (this.systemDark ? 'dark' : 'light') : this.mode
    // A theme without that mode shows its own default instead.
    return this.current.modes[pref] ? pref : this.current.defaultMode
  }

  get calm(): boolean {
    return this.motion === 'calm' || (this.motion === 'system' && this.systemCalm)
  }

  /** Call once at start. Applies saved choices and follows OS changes from then on. */
  start(store: PrefsStore = browserStore) {
    this.#store = store
    const saved = store.load()
    if (saved) {
      this.custom = saved.custom ?? {}
      this.theme = saved.theme in tokens.themes || saved.theme in this.custom ? saved.theme : defaultTheme
      this.mode = saved.mode ?? 'system'
      this.motion = saved.motion ?? 'system'
    }
    const dark = darkQuery()
    const calm = calmQuery()
    this.systemDark = dark.matches
    this.systemCalm = calm.matches
    dark.addEventListener('change', (e) => this.#transition(() => (this.systemDark = e.matches)))
    calm.addEventListener('change', (e) => (this.systemCalm = e.matches))
    $effect.root(() => {
      $effect(() => this.#apply())
    })
  }

  set(change: Partial<Pick<UiPrefs, 'theme' | 'mode' | 'motion'>>) {
    this.#transition(() => {
      if (change.theme !== undefined) this.theme = change.theme
      if (change.mode !== undefined) this.mode = change.mode
      if (change.motion !== undefined) this.motion = change.motion
    })
    this.#save()
  }

  /** Adds or replaces a user theme. Ids of built-in themes are refused. */
  saveCustom(id: string, def: ThemeDef) {
    if (id in tokens.themes) throw new Error(`"${id}" is a built-in theme`)
    this.custom = { ...this.custom, [id]: def }
    this.#save()
  }

  removeCustom(id: string) {
    const { [id]: _, ...rest } = this.custom
    this.custom = rest
    if (this.theme === id) this.theme = defaultTheme
    this.#save()
  }

  #save() {
    this.#store.save({ theme: this.theme, mode: this.mode, motion: this.motion, custom: $state.snapshot(this.custom) })
  }

  // Cross-fades the whole window between looks where the platform supports it, unless calm.
  #transition(change: () => void) {
    const doc = document as Document & { startViewTransition?: (cb: () => void) => unknown }
    if (this.calm || !doc.startViewTransition) change()
    else doc.startViewTransition(change)
  }

  #apply() {
    const root = document.documentElement
    root.dataset.theme = this.theme
    root.dataset.mode = this.resolvedMode
    root.dataset.motion = this.calm ? 'calm' : 'full'
    let style = document.getElementById('ui-custom-themes')
    if (!style) {
      style = document.createElement('style')
      style.id = 'ui-custom-themes'
      document.head.append(style)
    }
    style.textContent = Object.entries(this.custom)
      .flatMap(([id, def]) =>
        (['light', 'dark'] as const).map((m) => cssBlock(themeSelector(id, m), themeVars(tokens.fonts, def, m))),
      )
      .join('\n')
  }
}

export const ui = new Ui()

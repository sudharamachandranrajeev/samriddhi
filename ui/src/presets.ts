// Building blocks for the theme editor. Style choices come only from the built-in themes, so a
// user theme can mix them freely but can never inject arbitrary CSS.

import { tokens } from './generated/tokens.ts'
import { colourNames, styleNames, type ColourName, type Colours, type Mode, type StyleName, type ThemeDef } from './compile.ts'

export const styleGroups: Record<string, { label: string; help: string; keys: StyleName[] }> = {
  background: { label: 'Background', help: 'Plain colour, or soft coloured light behind everything.', keys: ['pageBg'] },
  cards: { label: 'Cards', help: 'How each box of information sits on the page.', keys: ['cardBg', 'cardBorder', 'cardShadow', 'cardBlur', 'glow'] },
  titles: { label: 'Card titles', help: 'Small capitals, or larger headings in the heading font.', keys: ['titleFont', 'titleSize', 'titleCase', 'titleTracking', 'titleColor'] },
  nav: { label: 'Menu highlight', help: 'How the page you are on is marked in the menu.', keys: ['navOnBg', 'navOnInk', 'navOnBar', 'navOnRadius'] },
  rows: { label: 'Lists', help: 'Filled rows, or ruled lines like a notebook.', keys: ['rowBg', 'rowRadius', 'rowDivider'] },
}

/** Each built-in theme offers its look for every group: [theme id, label]. */
export const presetChoices: [string, string][] = Object.entries(tokens.themes).map(([id, t]) => [id, t.label])

export function applyPreset(style: Record<StyleName, string>, group: string, themeId: string): Record<StyleName, string> {
  const from = tokens.themes[themeId].style
  const next = { ...style }
  for (const k of styleGroups[group].keys) next[k] = from[k]
  return next
}

/** Which built-in theme this group's values came from, or null if they match none. */
export function presetOf(style: Record<StyleName, string>, group: string): string | null {
  const keys = styleGroups[group].keys
  for (const [id, t] of Object.entries(tokens.themes)) if (keys.every((k) => t.style[k] === style[k])) return id
  return null
}

export const colourHelp: Record<ColourName, string> = {
  bg: 'Page background',
  glowA: 'Soft light, top right (Aurora background only)',
  glowB: 'Soft light, bottom left (Aurora background only)',
  panel: 'Cards and panels',
  panel2: 'Rows inside cards',
  ink: 'Main text',
  muted: 'Secondary text',
  line: 'Lines and borders',
  accent: 'Main colour: links, selected items, main buttons',
  onAccent: 'Text on main buttons',
  accent2: 'Second colour: gradients and highlights',
  accentSoft: 'Soft background behind main-colour labels',
  chart: 'Chart lines and dots',
  danger: 'Needs attention now',
  dangerSoft: 'Background behind "needs attention" labels',
  warn: 'Worth a look',
  warnSoft: 'Background behind "worth a look" labels',
  ok: 'Good or done',
  okSoft: 'Background behind "good" labels',
  focus: 'Keyboard focus ring',
  bodyFill: 'Body map: body colour',
  bodyLine: 'Body map: outline',
}

/** Plain words for a contrast pair, used when a pair fails. */
export function pairWords(fg: string, bg: string): string {
  return `${colourHelp[fg as ColourName]} on ${colourHelp[bg as ColourName].toLowerCase()}`
}

const HEX = /^#[0-9a-f]{6}$/i

/** Checks a theme that came from outside (import, old save). Returns the theme or a reason. */
export function validateTheme(input: unknown): ThemeDef | string {
  if (!input || typeof input !== 'object') return 'Not a theme.'
  const t = input as Partial<ThemeDef>
  if (typeof t.label !== 'string' || !t.label.trim() || t.label.length > 40) return 'The theme needs a name of up to 40 characters.'
  const modes: Partial<Record<Mode, Colours>> = {}
  for (const m of ['light', 'dark'] as const) {
    const c = t.modes?.[m]
    if (!c) continue
    for (const n of colourNames) if (typeof c[n] !== 'string' || !HEX.test(c[n])) return `Colour "${n}" (${m}) must look like #12ab9f.`
    modes[m] = Object.fromEntries(colourNames.map((n) => [n, c[n].toLowerCase()])) as Colours
  }
  if (!modes.light && !modes.dark) return 'The theme has no colours.'
  const defaultMode: Mode = t.defaultMode === 'dark' && modes.dark ? 'dark' : modes.light ? 'light' : 'dark'
  const fonts = Object.keys(tokens.fonts)
  const ty = t.type
  if (!ty || ![ty.body, ty.heading, ty.number].every((f) => fonts.includes(f))) return 'Unknown font.'
  const w = (n: unknown) => (typeof n === 'number' && n >= 300 && n <= 800 ? n : 600)
  const r = (n: unknown, max: number) => (typeof n === 'number' && n >= 0 && n <= max ? Math.round(n) : 12)
  const style = t.style as Record<StyleName, string> | undefined
  if (!style || !styleNames.every((k) => typeof style[k] === 'string')) return 'The theme is missing style settings.'
  for (const g of Object.keys(styleGroups)) if (!presetOf(style, g)) return `"${styleGroups[g].label}" is not one of the offered looks.`
  return {
    label: t.label.trim(),
    description: typeof t.description === 'string' ? t.description.slice(0, 120) : 'Your own theme.',
    defaultMode,
    type: { body: ty.body, heading: ty.heading, number: ty.number, headingWeight: w(ty.headingWeight), displayWeight: w(ty.displayWeight) },
    shape: { radius: r(t.shape?.radius, 28), radiusSmall: r(t.shape?.radiusSmall, 20), control: r(t.shape?.control, 20) },
    style: Object.fromEntries(styleNames.map((k) => [k, style[k]])) as Record<StyleName, string>,
    modes,
  }
}

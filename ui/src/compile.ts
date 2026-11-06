// Turns a theme definition into CSS custom properties. Used by the build script for the
// built-in themes and at runtime for themes made in the theme editor, so both stay identical.

export type Mode = 'light' | 'dark'

export const colourNames = [
  'bg', 'glowA', 'glowB', 'panel', 'panel2', 'ink', 'muted', 'line',
  'accent', 'onAccent', 'accent2', 'accentSoft', 'chart',
  'danger', 'dangerSoft', 'warn', 'warnSoft', 'ok', 'okSoft', 'focus',
  'bodyFill', 'bodyLine',
] as const
export type ColourName = (typeof colourNames)[number]
export type Colours = Record<ColourName, string>

export const styleNames = [
  'pageBg', 'cardBg', 'cardBorder', 'cardShadow', 'cardBlur',
  'titleFont', 'titleSize', 'titleCase', 'titleTracking', 'titleColor',
  'navOnBg', 'navOnInk', 'navOnBar', 'navOnRadius',
  'rowBg', 'rowRadius', 'rowDivider', 'glow',
] as const
export type StyleName = (typeof styleNames)[number]

export interface ThemeDef {
  label: string
  description: string
  defaultMode: Mode
  type: { body: string; heading: string; number: string; headingWeight: number; displayWeight: number }
  shape: { radius: number; radiusSmall: number; control: number }
  style: Record<StyleName, string>
  modes: Partial<Record<Mode, Colours>>
}

export interface Font { label: string; stack: string }

export interface Tokens {
  fonts: Record<string, Font>
  space: Record<string, number>
  text: Record<string, number>
  motion: { fast: number; base: number; slow: number; ease: string; spring: string }
  contrast: { pairs: [ColourName, ColourName, number][] }
  themes: Record<string, ThemeDef>
}

const kebab = (s: string) => s.replace(/[A-Z0-9]+/g, (m) => '-' + m.toLowerCase())

// `{name}` in a style value means the colour of that name; titleFont names a font role.
function resolveStyle(name: StyleName, value: string): string {
  if (name === 'titleFont') return `var(--font-${value})`
  return value.replace(/\{(\w+)\}/g, (_, c: string) => `var(--c-${kebab(c)})`)
}

export function fontStack(fonts: Record<string, Font>, id: string): string {
  return fonts[id]?.stack ?? fonts.inter.stack
}

/** Variables that do not change with the theme. */
export function baseVars(t: Tokens): Record<string, string> {
  const v: Record<string, string> = {}
  for (const [k, n] of Object.entries(t.space)) v[`--space-${k}`] = `${n}px`
  for (const [k, n] of Object.entries(t.text)) v[`--text-${k}`] = `${n}px`
  v['--dur-fast'] = `${t.motion.fast}ms`
  v['--dur-base'] = `${t.motion.base}ms`
  v['--dur-slow'] = `${t.motion.slow}ms`
  v['--ease'] = t.motion.ease
  v['--spring'] = t.motion.spring
  return v
}

/** Variables for one theme in one mode. Missing modes fall back to the theme's default mode. */
export function themeVars(fonts: Record<string, Font>, theme: ThemeDef, mode: Mode): Record<string, string> {
  const colours = theme.modes[mode] ?? theme.modes[theme.defaultMode]
  if (!colours) throw new Error(`theme ${theme.label} has no colours`)
  const v: Record<string, string> = { 'color-scheme': theme.modes[mode] ? mode : theme.defaultMode }
  for (const name of colourNames) v[`--c-${kebab(name)}`] = colours[name]
  v['--font-body'] = fontStack(fonts, theme.type.body)
  v['--font-heading'] = fontStack(fonts, theme.type.heading)
  v['--font-number'] = fontStack(fonts, theme.type.number)
  v['--weight-heading'] = String(theme.type.headingWeight)
  v['--weight-display'] = String(theme.type.displayWeight)
  v['--r'] = `${theme.shape.radius}px`
  v['--r-sm'] = `${theme.shape.radiusSmall}px`
  v['--r-control'] = `${theme.shape.control}px`
  for (const name of styleNames) v[`--${kebab(name)}`] = resolveStyle(name, theme.style[name])
  return v
}

export function cssBlock(selector: string, vars: Record<string, string>): string {
  const body = Object.entries(vars).map(([k, v]) => `  ${k}: ${v};`).join('\n')
  return `${selector} {\n${body}\n}\n`
}

// Not tied to :root, so any element can preview a theme with data-theme + data-mode.
export const themeSelector = (id: string, mode: Mode) => `[data-theme="${id}"][data-mode="${mode}"]`

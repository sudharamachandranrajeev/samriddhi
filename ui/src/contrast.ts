// WCAG 2.2 contrast ratio between two opaque hex colours (#rgb or #rrggbb).

function channel(c: number): number {
  const s = c / 255
  return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
}

export function parseHex(hex: string): [number, number, number] {
  let h = hex.trim().replace(/^#/, '')
  if (h.length === 3) h = [...h].map((x) => x + x).join('')
  if (!/^[0-9a-f]{6}$/i.test(h)) throw new Error(`not a hex colour: ${hex}`)
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number]
}

export function luminance(hex: string): number {
  const [r, g, b] = parseHex(hex).map(channel)
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

export function ratio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (hi + 0.05) / (lo + 0.05)
}

export interface Failure { fg: string; bg: string; need: number; got: number }

/** Every required pair that falls short, for one set of colours. */
export function check(colours: Record<string, string>, pairs: [string, string, number][]): Failure[] {
  const out: Failure[] = []
  for (const [fg, bg, need] of pairs) {
    const got = ratio(colours[fg], colours[bg])
    if (got < need) out.push({ fg, bg, need, got: Math.round(got * 100) / 100 })
  }
  return out
}

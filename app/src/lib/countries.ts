// Every country by its ISO 3166 two-letter code, with names in the user's
// language from the system (Intl.DisplayNames). UK, India and the UAE come
// first as quick picks.

const CODES = (
  'AD AE AF AG AI AL AM AO AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BW BY BZ ' +
  'CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM ' +
  'FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GT GU GW GY HK HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE ' +
  'JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN ' +
  'MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS ' +
  'PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TG TH TJ TK TL ' +
  'TM TN TO TR TT TV TW TZ UA UG US UY UZ VA VC VE VG VI VN VU WF WS XK YE YT ZA ZM ZW'
).split(' ')

export const QUICK = ['GB', 'IN', 'AE'] as const

let names: Intl.DisplayNames | null = null
const FALLBACK: Record<string, string> = { GB: 'United Kingdom', IN: 'India', AE: 'United Arab Emirates' }

export function countryName(code: string): string {
  try {
    names ??= new Intl.DisplayNames(['en-GB'], { type: 'region' })
    return names.of(code) ?? FALLBACK[code] ?? code
  } catch {
    return FALLBACK[code] ?? code
  }
}

export function allCountries(): { code: string; name: string }[] {
  return CODES.map((code) => ({ code, name: countryName(code) })).sort((a, b) => a.name.localeCompare(b.name))
}

/** Countries whose name or code starts a word with the search text, best first. */
export function searchCountries(query: string, limit = 8): { code: string; name: string }[] {
  const q = query.trim().toLowerCase()
  if (!q) return []
  const all = allCountries()
  const starts = all.filter((c) => c.name.toLowerCase().startsWith(q) || c.code.toLowerCase() === q)
  const words = all.filter((c) => !starts.includes(c) && c.name.toLowerCase().split(/[\s-]+/).some((w) => w.startsWith(q)))
  return [...starts, ...words].slice(0, limit)
}


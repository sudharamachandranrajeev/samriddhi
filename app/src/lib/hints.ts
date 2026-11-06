// One short learning hint per screen: a single idea worth knowing, in plain
// words. Each screen shows its hint; the home screen picks one by the day.

export const HINTS: Record<string, { title: string; text: string }> = {
  welcome: {
    title: 'Guidance or advice?',
    text: 'Regulated advice comes from a firm on the FCA register that looks at your whole situation and is responsible for its recommendation. Guidance, like this app, explains options so you can decide.',
  },
  protect: {
    title: 'Why a recovery key?',
    text: 'Your data is encrypted. Without the key nobody can read it, including you, so a written-down spare key is the only way back in if this device breaks.',
  },
  countries: {
    title: 'Residence is not citizenship',
    text: 'Where you are tax resident decides which country taxes your income. You can be a citizen of one country and resident in another; some people have ties to several.',
  },
  goals: {
    title: 'Goals change the order',
    text: 'The same money can pay off a card, fill a cushion or go into a pension. Your goals decide which comes first, and the right order often saves more than any investment earns.',
  },
  money: {
    title: 'Rough is fine',
    text: 'A figure to the nearest hundred is enough to spot the big things, like a missing cushion or costly debt. You can make it exact later, or never.',
  },
  risk: {
    title: 'Willing and able',
    text: 'How much risk you are willing to take is your attitude; how much loss you could absorb is your capacity. A good plan respects the lower of the two.',
  },
  done: {
    title: 'One step at a time',
    text: 'Small steps you actually finish beat a perfect plan you never start. Samriddhi shows one at a time and explains each.',
  },
  unlock: {
    title: 'The PIN guards the screen',
    text: 'Your PIN stops someone using the open app. The data itself is protected by the encryption key this device keeps, whatever the PIN.',
  },
  recover: {
    title: 'Keys stay on the device',
    text: 'The key that opens your data lives in this device’s secure key store. A new device, or a reset, needs the recovery key you wrote down at set-up.',
  },
  settings: {
    title: 'Your data, your copy',
    text: 'An export is a readable copy of everything, for you to keep or move. A backup is an encrypted copy that only your keys can open.',
  },
}

const HOME_HINTS = [
  {
    title: 'Emergency cushion',
    text: 'Three to six months of costs in an easy-access account means a surprise bill or a gap in pay does not turn into debt.',
  },
  {
    title: 'Expensive debt first',
    text: 'Paying off a card at 20% a year is a certain 20% return. Few investments can promise that, so costly debt usually comes before investing.',
  },
  {
    title: 'Allowances reset',
    text: 'Tax-free allowances, like the UK ISA allowance, reset each tax year. What you do not use by the deadline is lost, not carried over.',
  },
  {
    title: 'Net worth',
    text: 'Net worth is what you own minus what you owe. It moves slowly; the direction over a year matters more than any one day.',
  },
  {
    title: 'Too good to be true',
    text: 'Guaranteed high returns are the most common sign of a scam. A real firm can be checked on the FCA register and its Warning List.',
  },
]

/** The home screen's hint for a day (UTC date), so it changes daily but not on every visit. */
export function homeHint(day: string): { title: string; text: string } {
  let n = 0
  for (const ch of day) n = (n * 31 + ch.charCodeAt(0)) % 9973
  return HOME_HINTS[n % HOME_HINTS.length]
}

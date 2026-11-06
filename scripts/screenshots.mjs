// Renders the README screenshots: every wizard screen, home and settings, in
// each built-in theme. It runs the app in a browser, where the Rust shell is
// replaced by the in-memory stand-in (app/src/lib/mock.ts), so every figure
// is made up and nothing is read from or written to a real store.
//
//   SAMRIDDHI_SHOTS=1 node scripts/screenshots.mjs
//
// SAMRIDDHI_SHOTS=1 writes to docs/screenshots/; any other value is taken
// as the folder to write to. CHROMIUM=/path/to/chromium if it is not
// /usr/bin/chromium.
import { mkdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { chromium } from 'playwright-core'
import { createServer } from 'vite'

const target = process.env.SAMRIDDHI_SHOTS
if (!target) {
  console.error('Set SAMRIDDHI_SHOTS=1 (or to an output folder) to render the screenshots.')
  process.exit(2)
}
const root = fileURLToPath(new URL('..', import.meta.url))
const OUT = target === '1' ? `${root}docs/screenshots/` : target.replace(/\/?$/, '/')
mkdirSync(OUT, { recursive: true })

const server = await createServer({ root: `${root}app`, configFile: `${root}app/vite.config.ts`, logLevel: 'error', server: { port: 5199, strictPort: true } })
await server.listen()
const BASE = 'http://localhost:5199/'

const browser = await chromium.launch({ executablePath: process.env.CHROMIUM ?? '/usr/bin/chromium' })
const errors = []
const THEMES = { swarna: 'dark', bahi: 'light', vana: 'light' }

async function open(theme, mode, search = '') {
  const page = await browser.newPage({ viewport: { width: 1280, height: 820 }, colorScheme: mode, reducedMotion: 'reduce' })
  page.on('pageerror', (e) => errors.push(`${theme}: ${e.message}`))
  await page.addInitScript((prefs) => localStorage.setItem('samriddhi.ui', JSON.stringify(prefs)), { theme, mode, motion: 'calm', custom: {} })
  await page.goto(BASE + search)
  await page.evaluate(() => document.fonts.ready)
  return page
}

const button = (page, name) => page.getByRole('button', { name, exact: true })
async function shot(page, name) {
  await page.waitForTimeout(150)
  await page.evaluate(() => document.fonts.ready)
  // Grow the window to the page instead of a full-page capture, so the
  // sticky side rail and the backdrop reach the bottom as they would on screen.
  const { width } = page.viewportSize()
  const height = await page.evaluate(() => document.documentElement.scrollHeight)
  await page.setViewportSize({ width, height: Math.max(820, height) })
  await page.waitForTimeout(100)
  await page.screenshot({ path: `${OUT}${name}.png` })
  await page.setViewportSize({ width, height: 820 })
}

for (const [theme, mode] of Object.entries(THEMES)) {
  const page = await open(theme, mode)
  const p = `${theme}-`

  await page.getByRole('heading', { name: 'Welcome to Samriddhi' }).waitFor()
  await shot(page, `${p}wizard-1-welcome`)
  await page.getByLabel('I have read these six points and accept them.').check()
  await button(page, 'Accept and continue').click()

  await button(page, 'Create my encrypted store').click()
  await page.locator('code.key').filter({ hasText: /[0-9a-f]{8}/ }).waitFor()
  await shot(page, `${p}wizard-2-protect`)
  await page.getByLabel('I have written down or saved my recovery key.').check()
  await button(page, 'Continue').click()

  await page.getByRole('heading', { name: 'Your countries' }).waitFor()
  await button(page, 'United Kingdom').click()
  await button(page, 'India').click()
  await page.locator('fieldset', { hasText: 'India' }).getByLabel('I am a citizen').check()
  await shot(page, `${p}wizard-3-countries`)
  await button(page, 'Save and continue').click()

  await page.getByRole('heading', { name: 'What you want most' }).waitFor()
  await page.getByRole('button', { name: /^Emergency cushion/ }).click()
  await page.getByRole('button', { name: /^Clear debt/ }).click()
  await page.fill('#goal-emergencyCushion-amount', '13,000')
  await shot(page, `${p}wizard-4-goals`)
  await button(page, 'Save and continue').click()

  await page.getByRole('heading', { name: 'Money in and out' }).waitFor()
  await page.fill('#take-home', '3,100')
  await page.fill('#costs', '2,150')
  await page.fill('#cash', '4,200')
  await page.getByLabel('Yes', { exact: true }).check()
  await page.fill('#debt-0-label', 'Credit card')
  await page.fill('#debt-0-balance', '1,800')
  await page.selectOption('#debt-0-band', 'over20')
  await shot(page, `${p}wizard-5-money`)
  await button(page, 'Save and continue').click()

  await page.getByRole('heading', { name: 'How you feel about risk' }).first().waitFor()
  await page.locator('fieldset.q').first().waitFor()
  for (const q of await page.locator('fieldset.q').all()) {
    const radios = q.getByRole('radio')
    const n = await radios.count()
    await radios.nth(n === 5 ? 2 : 1).check()
  }
  await shot(page, `${p}wizard-6-risk`)
  await button(page, 'See my result').click()
  await page.getByText('Suggestions will keep to').waitFor()
  await shot(page, `${p}wizard-6-risk-result`)
  await button(page, 'Continue').click()

  await page.getByRole('heading', { name: 'Your first step' }).waitFor()
  await page.locator('article.step').waitFor()
  await shot(page, `${p}wizard-7-done`)
  await button(page, 'Go to my home').click()

  await page.getByRole('heading', { name: /^Good (morning|afternoon|evening)$/ }).waitFor()
  await page.locator('article.step').first().waitFor()
  await shot(page, `${p}home`)

  await page.evaluate(() => (location.hash = 'settings'))
  await page.getByRole('heading', { name: 'Settings' }).waitFor()
  await shot(page, `${p}settings`)
  await page.getByRole('radio', { name: 'Backups' }).click()
  await page.getByText('Kept in').waitFor()
  await shot(page, `${p}settings-backups`)
  await page.close()

  // The same home in the theme's other mode, from a finished set-up.
  const other = mode === 'dark' ? 'light' : 'dark'
  const second = await open(theme, other, '?demo=home')
  await second.locator('article.step').first().waitFor()
  await shot(second, `${p}home-${other}`)
  await second.close()
}

// Lock screen and phone width, Swarna only.
const lock = await open('swarna', 'dark', '?demo=locked')
await lock.getByRole('heading', { name: 'Samriddhi is locked' }).waitFor()
await shot(lock, 'swarna-unlock')
await lock.close()
const phone = await browser.newPage({ viewport: { width: 390, height: 844 }, colorScheme: 'dark', reducedMotion: 'reduce' })
await phone.addInitScript(() => localStorage.setItem('samriddhi.ui', JSON.stringify({ theme: 'swarna', mode: 'dark', motion: 'calm', custom: {} })))
phone.on('pageerror', (e) => errors.push(`phone: ${e.message}`))
await phone.goto(`${BASE}?demo=home`)
await phone.locator('article.step').first().waitFor()
await phone.evaluate(() => document.fonts.ready)
await phone.screenshot({ path: `${OUT}swarna-phone.png` })

await browser.close()
await server.close()
if (errors.length) {
  console.error(errors.join('\n'))
  process.exit(1)
}
console.log(`Saved to ${OUT}`)

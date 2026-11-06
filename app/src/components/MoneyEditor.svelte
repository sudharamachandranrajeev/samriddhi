<script lang="ts">
  // Rough monthly take-home and costs, cash savings, and debts with rate
  // bands. Every box is optional: "rough is fine", and skipped ones are
  // asked again later, one at a time.
  import { Button, Explain } from '@samriddhi/ui'
  import Plus from '@lucide/svelte/icons/plus'
  import X from '@lucide/svelte/icons/x'
  import { call, type Profile, type Source } from '../lib/api.ts'
  import { MAIN_CURRENCIES, currencyFor, formatMoney, roughNetWorth, toInput } from '../lib/money.ts'
  import { RATE_BANDS, buildMoney, type DebtDraft, type MoneyDraft } from '../lib/wizard.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import MoneyField from './MoneyField.svelte'

  let { source, saveLabel, onsaved }: { source: Source; saveLabel: string; onsaved: () => void } = $props()

  let d = $state<MoneyDraft>({ currency: 'GBP', takeHome: '', costs: '', cash: '', debts: null })
  let had: Profile | null = null
  let error = $state('')
  let busy = $state(false)

  const built = $derived(buildMoney(d))
  const preview = $derived('facts' in built ? roughNetWorth(built.facts.cash, built.facts.debts) : null)
  const currencies = $derived([...new Set([...MAIN_CURRENCIES, d.currency])])

  call('profile_get')
    .then(({ profile: p }) => {
      had = p
      d = {
        currency: p.cash?.currency ?? p.costs?.currency ?? p.takeHome?.currency ?? currencyFor(p.countries.find((c) => c.taxHome)?.code),
        takeHome: toInput(p.takeHome),
        costs: toInput(p.costs),
        cash: toInput(p.cash),
        debts: p.debts?.map((x) => ({ label: x.label, balance: toInput(x.balance), band: x.band })) ?? null,
      }
    })
    .catch(() => {})

  function owes(v: 'yes' | 'no' | 'skip') {
    if (v === 'no') d.debts = []
    else if (v === 'skip') d.debts = null
    else if (!d.debts?.length) d.debts = [{ label: '', balance: '', band: 'unknown' }]
  }
  const owing = $derived(d.debts === null ? 'skip' : d.debts.length ? 'yes' : 'no')

  function addDebt() {
    d.debts = [...(d.debts ?? []), { label: '', balance: '', band: 'unknown' } satisfies DebtDraft]
  }
  function removeDebt(i: number) {
    d.debts = (d.debts ?? []).filter((_, j) => j !== i)
  }

  async function save() {
    if ('error' in built) {
      error = built.error
      return
    }
    error = ''
    busy = true
    const f = built.facts
    const pairs: [string, unknown, unknown][] = [
      ['money.takeHomeMonthly', f.takeHome, had?.takeHome],
      ['money.costsMonthly', f.costs, had?.costs],
      ['money.cashSavings', f.cash, had?.cash],
      ['money.debts', f.debts, had?.debts],
    ]
    try {
      for (const [key, value, before] of pairs) {
        if (value !== null) await call('set_fact', { key, value, source })
        else if (before) await call('clear_fact', { key })
      }
      onsaved()
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }
</script>

<div class="stack">
  <p class="note-box">Rough is fine: to the nearest hundred is plenty. Leave a box empty to skip it for now.</p>

  <div class="field cur">
    <label for="money-currency">Currency for these amounts</label>
    <select id="money-currency" class="input" bind:value={d.currency}>
      {#each currencies as c (c)}<option value={c}>{c}</option>{/each}
    </select>
  </div>

  <div class="three">
    <MoneyField id="take-home" label="Take-home pay a month" help="after tax" currency={d.currency} bind:value={d.takeHome} />
    <MoneyField id="costs" label="Costs a month" help="rent, bills, food, travel" currency={d.currency} bind:value={d.costs} />
    <MoneyField id="cash" label="Cash savings" help="money you can reach within days" currency={d.currency} bind:value={d.cash} />
  </div>

  <fieldset>
    <legend>Do you owe money? (not counting a mortgage)</legend>
    <div class="row-actions">
      <label class="choice"><input type="radio" name="owes" checked={owing === 'yes'} onchange={() => owes('yes')} /> Yes</label>
      <label class="choice"><input type="radio" name="owes" checked={owing === 'no'} onchange={() => owes('no')} /> No debts</label>
      <label class="choice"><input type="radio" name="owes" checked={owing === 'skip'} onchange={() => owes('skip')} /> Skip for now</label>
    </div>
  </fieldset>

  {#if d.debts?.length}
    {#each d.debts as debt, i (i)}
      <fieldset class="debt">
        <legend>Debt {i + 1}</legend>
        <div class="field">
          <label for={`debt-${i}-label`}>Name</label>
          <input id={`debt-${i}-label`} class="input" placeholder="Credit card" maxlength="80" bind:value={debt.label} />
        </div>
        <MoneyField id={`debt-${i}-balance`} label="Owed now" currency={d.currency} bind:value={debt.balance} />
        <div class="field">
          <label for={`debt-${i}-band`}>Interest a year</label>
          <select id={`debt-${i}-band`} class="input" bind:value={debt.band}>
            {#each RATE_BANDS as b (b.band)}<option value={b.band}>{b.label}</option>{/each}
          </select>
        </div>
        <button class="remove" aria-label={`Remove debt ${i + 1}`} onclick={() => removeDebt(i)}><X size={16} /></button>
      </fieldset>
    {/each}
    <div><Button variant="ghost" onclick={addDebt}><Plus size={16} /> Add another debt</Button></div>
  {/if}

  <div class="preview" aria-live="polite">
    <span class="muted small">Rough net worth, from these answers</span>
    <Explain title="Rough net worth" text="Your cash savings minus what you owe, using only the figures on this screen. It leaves out homes, pensions and investments for now, so it is a starting point, not your full picture." />
    <span class="num">{preview ? formatMoney(preview) : 'Fill in cash and debts to see it'}</span>
  </div>

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="row-actions"><Button onclick={save} disabled={busy}>{saveLabel}</Button></div>
</div>

<style>
  .cur { max-width: 220px; }
  .three { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: var(--space-md); }
  .debt { position: relative; display: grid; grid-template-columns: 1.2fr 1fr 1fr 32px; gap: var(--space-md); align-items: start; border-top: 1px solid var(--c-line); padding-top: var(--space-md); }
  .debt legend { grid-column: 1 / -1; }
  .remove { margin-top: 28px; border: 0; background: none; color: var(--c-muted); cursor: pointer; padding: 6px; border-radius: var(--r-sm); }
  .remove:hover { color: var(--c-danger); }
  .preview { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; padding: var(--space-md) var(--space-lg); border-radius: var(--r-sm); background: var(--c-panel-2); }
  .preview .num { margin-left: auto; font-weight: 700; font-size: var(--text-lead); }
  @media (max-width: 640px) { .debt { grid-template-columns: 1fr 32px; } .debt > :not(legend):not(.remove) { grid-column: 1; } .remove { grid-column: 2; grid-row: 2; } }
</style>

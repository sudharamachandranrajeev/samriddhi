<script lang="ts">
  // Add or change one holding: a name, its kind of asset, and the amount in
  // its own currency. Amounts are exact; nothing is converted.
  import { Button } from '@samriddhi/ui'
  import { call, type AssetClass, type Holding } from '../lib/api.ts'
  import { MAIN_CURRENCIES, parseAmount, toInput } from '../lib/money.ts'
  import { CLASSES } from '../lib/portfolio.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import MoneyField from './MoneyField.svelte'

  let {
    accountId,
    holding,
    base,
    onsaved,
    oncancel,
  }: { accountId: string; holding?: Holding; base: string; onsaved: () => void; oncancel: () => void } = $props()

  // svelte-ignore state_referenced_locally
  const start = holding
  let name = $state(start?.name ?? '')
  let assetClass = $state<AssetClass>(start?.assetClass ?? 'equity')
  // svelte-ignore state_referenced_locally
  let currency = $state(start?.amount.currency ?? base)
  let amount = $state(toInput(start?.amount))
  let error = $state('')
  let busy = $state(false)
  const currencies = $derived([...new Set([base, ...MAIN_CURRENCIES, currency])])
  // svelte-ignore state_referenced_locally
  const id = `h-${start?.id ?? `new-${accountId}`}`

  async function save(e: SubmitEvent) {
    e.preventDefault()
    if (!name.trim()) {
      error = 'Give the holding a short name, such as the fund or account.'
      return
    }
    const parsed = parseAmount(amount, currency)
    if (!parsed.ok) {
      error = amount.trim() ? parsed.error : 'Type the amount it is worth now.'
      return
    }
    error = ''
    busy = true
    try {
      if (start) await call('holding_update', { holding: { ...start, name: name.trim(), assetClass, amount: parsed.money } })
      else await call('holding_add', { accountId, name: name.trim(), assetClass, amount: parsed.money })
      onsaved()
    } catch (err) {
      toast.error(err)
    } finally {
      busy = false
    }
  }
</script>

<form class="holding-form" onsubmit={save} aria-label={start ? `Change ${start.name}` : 'Add a holding'}>
  <div class="field"><label for={`${id}-name`}>Name</label><input id={`${id}-name`} class="input" maxlength="80" autocomplete="off" bind:value={name} placeholder="For example: Global index fund" /></div>
  <div class="field">
    <label for={`${id}-class`}>Kind of asset</label>
    <select id={`${id}-class`} class="input" bind:value={assetClass}>
      {#each CLASSES as c (c.id)}<option value={c.id}>{c.label}</option>{/each}
    </select>
  </div>
  <div class="field">
    <label for={`${id}-cur`}>Currency</label>
    <select id={`${id}-cur`} class="input" bind:value={currency}>
      {#each currencies as c (c)}<option value={c}>{c}</option>{/each}
    </select>
  </div>
  <MoneyField id={`${id}-amount`} label="Worth now" {currency} bind:value={amount} help={currency === base ? '' : `Not counted in totals until exchange rates arrive`} />
  {#if error}<p class="error wide" role="alert">{error}</p>{/if}
  <div class="row-actions wide">
    <Button type="submit" disabled={busy}>{start ? 'Save changes' : 'Add holding'}</Button>
    <Button variant="ghost" onclick={oncancel}>Cancel</Button>
  </div>
</form>

<style>
  .holding-form { display: grid; gap: var(--space-md); grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); padding: var(--space-md); border: 1px dashed var(--c-line); border-radius: var(--r-sm); }
  .wide { grid-column: 1 / -1; }
</style>

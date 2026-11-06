<script lang="ts">
  // Wealth: your accounts and the holdings in them, added up by kind of
  // asset in one currency. Holdings in other currencies are listed and
  // named, but not counted until exchange rates arrive. Every figure has a "?".
  import { Button, Card, Explain, Pill, Stat } from '@samriddhi/ui'
  import Plus from '@lucide/svelte/icons/plus'
  import { call, type AccountView, type Holding, type WealthView, type Wrapper } from '../lib/api.ts'
  import { HINTS } from '../lib/hints.ts'
  import { formatMoney } from '../lib/money.ts'
  import { CLASSES, classLabel, pct, WRAPPERS, wrapperLabel } from '../lib/portfolio.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import Hint from '../components/Hint.svelte'
  import HoldingForm from '../components/HoldingForm.svelte'
  import MixBar from '../components/MixBar.svelte'

  let view = $state<WealthView | null>(null)
  /** What is open for editing: 'new-account', 'account:<id>', 'add:<accountId>' or 'holding:<id>'. */
  let editing = $state<string | null>(null)
  let confirming = $state<string | null>(null)
  let accName = $state('')
  let accWrapper = $state<Wrapper>('isa')
  let busy = $state(false)

  async function load() {
    try {
      view = await call('wealth_view')
    } catch (e) {
      toast.error(e)
    }
  }
  load()

  function open(key: string, a?: AccountView) {
    editing = key
    confirming = null
    accName = a?.account.name ?? ''
    accWrapper = a?.account.wrapper ?? 'isa'
  }

  async function done(message: string) {
    editing = null
    confirming = null
    toast.show(message)
    await load()
  }

  async function saveAccount(e: SubmitEvent, id?: string) {
    e.preventDefault()
    if (!accName.trim()) {
      toast.show('Give the account a short name, such as “Stocks and shares ISA”.', 'danger')
      return
    }
    busy = true
    try {
      if (id) await call('account_update', { id, name: accName.trim(), wrapper: accWrapper })
      else await call('account_add', { name: accName.trim(), wrapper: accWrapper })
      await done(id ? 'Account saved.' : 'Account added. Now add what is in it.')
    } catch (err) {
      toast.error(err)
    } finally {
      busy = false
    }
  }

  async function remove(kind: 'account' | 'holding', id: string) {
    try {
      if (kind === 'account') await call('account_delete', { id })
      else await call('holding_delete', { id })
      await done(kind === 'account' ? 'Account and its holdings removed.' : 'Holding removed.')
    } catch (e) {
      toast.error(e)
    }
  }

  const t = $derived(view?.totals)
  const counted = (h: Holding) => h.amount.currency === view?.base
</script>

<header class="page-head">
  <div>
    <h1 tabindex="-1">Wealth</h1>
    <p>Your accounts and what is in them, added up by kind of asset, so the plan can compare what you have with the mix it suggests.</p>
  </div>
</header>

<div class="grid wealth">
  <Card title="What your holdings add up to" explain="Every holding you entered or imported, added up in your main currency and split by kind of asset. It is what you hold, not net worth: debts are not taken off here.">
    {#if !view || !t}
      <p class="muted">Loading…</p>
    {:else}
      <div class="big-figure" aria-describedby="w-note">{formatMoney(t.total)}</div>
      <p id="w-note" class="muted small rough">In {view.base}, from {view.accounts.length} account{view.accounts.length === 1 ? '' : 's'}. Figures are what you typed or imported, not live prices.</p>
      {#if view.leftOut.length}
        <p class="note-box warn" role="status">
          Not counted, no exchange rates yet: {t.notCountedNoFx.length} holding{t.notCountedNoFx.length === 1 ? '' : 's'} in {view.leftOut.join(', ')}.
          They are kept and listed below, and will count once Samriddhi has dated exchange rates.
        </p>
      {/if}
      <div class="stats">
        {#each CLASSES as c (c.id)}
          <Stat label={c.label} value={formatMoney(t.byClass[c.id])} note={t.total.minor > 0 ? pct(t.weights[c.id]) : ''} explain={c.explain} />
        {/each}
      </div>
      {#if t.total.minor > 0}
        <div class="mix"><MixBar weights={t.weights} label="Your mix now" /></div>
      {/if}
    {/if}
  </Card>

  <Card title="Accounts" explain="An account is where holdings sit: an ISA, a pension, a taxable account or a cash account. The kind matters for tax, and for where rebalancing costs least.">
    <div class="stack">
      {#if view && !view.accounts.length}
        <p class="muted">No accounts yet. Add one by hand, or import a holdings file in Settings, Import holdings.</p>
      {/if}
      {#each view?.accounts ?? [] as a (a.account.id)}
        <section class="account" aria-labelledby={`acc-${a.account.id}`}>
          {#if editing === `account:${a.account.id}`}
            <form class="acc-form" onsubmit={(e) => saveAccount(e, a.account.id)} aria-label={`Change ${a.account.name}`}>
              <div class="field"><label for={`acc-name-${a.account.id}`}>Account name</label><input id={`acc-name-${a.account.id}`} class="input" maxlength="80" bind:value={accName} /></div>
              <div class="field"><label for={`acc-wrap-${a.account.id}`}>Kind of account</label><select id={`acc-wrap-${a.account.id}`} class="input" bind:value={accWrapper}>{#each WRAPPERS as w (w.value)}<option value={w.value}>{w.label}</option>{/each}</select></div>
              <div class="row-actions wide"><Button type="submit" disabled={busy}>Save</Button><Button variant="ghost" onclick={() => (editing = null)}>Cancel</Button></div>
            </form>
          {:else}
            <div class="acc-head">
              <div>
                <h3 id={`acc-${a.account.id}`}>{a.account.name}</h3>
                <p class="tags"><Pill tone="accent">{wrapperLabel(a.account.wrapper)}</Pill>{#if a.account.source === 'import'}<Pill tone="done">Imported</Pill>{/if}</p>
              </div>
              <div class="acc-total">
                <span class="num">{formatMoney(a.counted)}</span>
                <Explain title="Account total" text={`What the holdings in ${view?.base} add up to.${a.notCounted ? ` ${a.notCounted} in another currency ${a.notCounted === 1 ? 'is' : 'are'} not counted yet.` : ''}`} />
              </div>
            </div>
          {/if}

          <ul class="holdings">
            {#each a.holdings as h (h.id)}
              <li>
                {#if editing === `holding:${h.id}`}
                  <div class="h-edit"><HoldingForm accountId={a.account.id} holding={h} base={view?.base ?? 'GBP'} onsaved={() => done('Holding saved.')} oncancel={() => (editing = null)} /></div>
                {:else}
                  <span class="h-name">{h.name}<small class="muted">{classLabel(h.assetClass)}</small></span>
                  <span class="h-amount num">{formatMoney(h.amount)}{#if !counted(h)}<small class="warn-text">not counted, no FX yet</small>{/if}</span>
                  <span class="row-actions h-actions">
                    <Button variant="ghost" onclick={() => open(`holding:${h.id}`)}>Edit<span class="sr-only"> {h.name}</span></Button>
                    {#if confirming === `holding:${h.id}`}
                      <Button variant="quiet" onclick={() => remove('holding', h.id)}>Remove it</Button>
                      <Button variant="ghost" onclick={() => (confirming = null)}>Keep</Button>
                    {:else}
                      <Button variant="ghost" onclick={() => (confirming = `holding:${h.id}`)}>Remove<span class="sr-only"> {h.name}</span></Button>
                    {/if}
                  </span>
                {/if}
              </li>
            {:else}
              <li class="muted small">Nothing in this account yet.</li>
            {/each}
          </ul>

          {#if editing === `add:${a.account.id}`}
            <HoldingForm accountId={a.account.id} base={view?.base ?? 'GBP'} onsaved={() => done('Holding added.')} oncancel={() => (editing = null)} />
          {:else if editing !== `account:${a.account.id}`}
            <div class="row-actions">
              <Button variant="quiet" onclick={() => open(`add:${a.account.id}`)}>Add a holding<span class="sr-only"> to {a.account.name}</span></Button>
              <Button variant="ghost" onclick={() => open(`account:${a.account.id}`, a)}>Rename or change kind<span class="sr-only"> of {a.account.name}</span></Button>
              {#if confirming === `account:${a.account.id}`}
                <span class="small">Remove {a.account.name} and its {a.holdings.length} holding{a.holdings.length === 1 ? '' : 's'}?</span>
                <Button variant="quiet" onclick={() => remove('account', a.account.id)}>Remove account</Button>
                <Button variant="ghost" onclick={() => (confirming = null)}>Keep</Button>
              {:else}
                <Button variant="ghost" onclick={() => (confirming = `account:${a.account.id}`)}>Remove account<span class="sr-only"> {a.account.name}</span></Button>
              {/if}
            </div>
          {/if}
        </section>
      {/each}

      {#if editing === 'new-account'}
        <form class="acc-form" onsubmit={(e) => saveAccount(e)} aria-label="Add an account">
          <div class="field"><label for="acc-name-new">Account name</label><input id="acc-name-new" class="input" maxlength="80" bind:value={accName} placeholder="For example: Stocks and shares ISA" /></div>
          <div class="field"><label for="acc-wrap-new">Kind of account</label><select id="acc-wrap-new" class="input" bind:value={accWrapper}>{#each WRAPPERS as w (w.value)}<option value={w.value}>{w.label}</option>{/each}</select></div>
          <div class="row-actions wide"><Button type="submit" disabled={busy}>Add account</Button><Button variant="ghost" onclick={() => (editing = null)}>Cancel</Button></div>
        </form>
      {:else}
        <div><Button onclick={() => open('new-account')}><Plus size={16} aria-hidden="true" /> Add an account</Button></div>
      {/if}
    </div>
  </Card>
</div>

<Hint {...HINTS.wealth} />

<style>
  .wealth { grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); }
  .rough { margin: 6px 0 var(--space-lg); }
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: var(--space-sm); margin-top: var(--space-md); }
  .mix { margin-top: var(--space-lg); }
  .account { border-bottom: 1px solid var(--c-line); padding-bottom: var(--space-md); display: flex; flex-direction: column; gap: var(--space-sm); }
  .acc-head { display: flex; justify-content: space-between; gap: var(--space-md); align-items: flex-start; }
  .acc-head h3 { font-size: var(--text-title); line-height: 1.3; }
  .tags { display: flex; gap: 6px; margin-top: 4px; flex-wrap: wrap; }
  .acc-total { display: inline-flex; gap: 6px; align-items: center; font-weight: 600; white-space: nowrap; }
  .acc-form { display: grid; gap: var(--space-md); grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); padding: var(--space-md); border: 1px dashed var(--c-line); border-radius: var(--r-sm); }
  .wide { grid-column: 1 / -1; }
  .holdings { list-style: none; display: flex; flex-direction: column; }
  .holdings li { display: flex; flex-wrap: wrap; gap: 4px var(--space-md); align-items: center; padding: 8px 0; border-top: 1px solid var(--c-line); }
  .h-edit { flex: 1 1 100%; }
  .h-name { flex: 1 1 160px; min-width: 0; overflow-wrap: anywhere; }
  .h-name small, .h-amount small { display: block; font-size: var(--text-caption); }
  .h-amount { text-align: right; flex: 0 0 auto; }
  .warn-text { color: var(--c-warn); font-family: var(--font-body); }
  .h-actions { flex: 1 0 100%; justify-content: flex-end; }
  @media (min-width: 1100px) { .h-actions { flex: 0 0 auto; } }
</style>

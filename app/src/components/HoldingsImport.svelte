<script lang="ts">
  // Import holdings from a JSON or CSV file: pick it, see how many accounts
  // and holdings it would add or change, then confirm. An import adds or
  // updates; it never deletes, and the current data is backed up first.
  import { Button, Card, Explain } from '@samriddhi/ui'
  import { call, type HoldingsImportView } from '../lib/api.ts'
  import { toast } from '../lib/toast.svelte.ts'

  let { onimported }: { onimported?: () => void } = $props()

  let picked = $state<HoldingsImportView | null>(null)
  let busy = $state(false)

  async function pick() {
    busy = true
    try {
      picked = await call('holdings_import_pick')
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }

  async function apply() {
    busy = true
    try {
      const r = await call('holdings_import_apply')
      toast.show(`Imported: ${r.done.newHoldings} new and ${r.done.updatedHoldings} changed holdings. Your data from before was kept as backup ${r.backup}.`)
      picked = null
      onimported?.()
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }

  async function cancel() {
    await call('holdings_import_cancel').catch(() => {})
    picked = null
  }

  const p = $derived(picked?.preview)
</script>

<div class="grid">
  <Card title="Import holdings" explain="Read accounts and holdings from a file, for example an export from another app: JSON in the samriddhi-holdings-v1 format, or CSV with the columns account, wrapper, holding, assetClass, value and an optional currency. Samriddhi only reads the file.">
    <div class="stack">
      {#if !picked || !p}
        <p class="small">You see what the file would add or change before anything is saved. An import never deletes; importing the same file again updates the same rows.</p>
        <div><Button variant="quiet" disabled={busy} onclick={pick}>Choose a holdings file…</Button></div>
      {:else}
        <p class="small">From <span class="num">{picked.file}</span>: {picked.accounts} account{picked.accounts === 1 ? '' : 's'} with {picked.holdings} holding{picked.holdings === 1 ? '' : 's'}. Rows without a currency are read as {picked.base}.</p>
        <table class="counts">
          <caption class="sr-only">What importing would change</caption>
          <thead><tr><th scope="col">What</th><th scope="col" class="n">New</th><th scope="col" class="n">Changed</th><th scope="col" class="n">Same</th></tr></thead>
          <tbody>
            <tr><td>Accounts</td><td class="n">{p.newAccounts}</td><td class="n">{p.updatedAccounts}</td><td class="n">{p.unchangedAccounts}</td></tr>
            <tr><td>Holdings</td><td class="n">{p.newHoldings}</td><td class="n">{p.updatedHoldings}</td><td class="n">{p.unchangedHoldings}</td></tr>
          </tbody>
        </table>
        {#if p.notCountedNoFx}
          <p class="note-box warn small">{p.notCountedNoFx} holding{p.notCountedNoFx === 1 ? ' is' : 's are'} in another currency than {picked.base}: kept, but not counted in totals until exchange rates arrive.</p>
        {/if}
        {#if p.problems.length}
          <div class="small">
            <p><strong>{p.problems.length} row{p.problems.length === 1 ? '' : 's'} will be skipped</strong> <Explain title="Skipped rows" text="Rows with an unknown asset class or account kind, a missing name, or an amount that is not a plain number at or above zero. The reasons never repeat the file's own values." /></p>
            <ul class="problems">{#each p.problems.slice(0, 8) as why, i (i)}<li>{why}</li>{/each}</ul>
            {#if p.problems.length > 8}<p class="muted">and {p.problems.length - 8} more.</p>{/if}
          </div>
        {/if}
        <p class="note-box small">Your current data is backed up before the import. Nothing is deleted.</p>
        <div class="row-actions">
          <Button disabled={busy || p.newHoldings + p.updatedHoldings + p.newAccounts + p.updatedAccounts === 0} onclick={apply}>Import these holdings</Button>
          <Button variant="ghost" onclick={cancel}>Cancel</Button>
        </div>
      {/if}
    </div>
  </Card>
</div>

<style>
  .problems { padding-left: 1.2em; margin-top: 4px; color: var(--c-muted); }
</style>

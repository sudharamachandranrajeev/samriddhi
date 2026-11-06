<script lang="ts">
  // Where you live, pay tax, hold money and are a citizen. Quick picks for
  // the UK, India and the UAE, and a search over every country.
  import { Button } from '@samriddhi/ui'
  import X from '@lucide/svelte/icons/x'
  import { call, type CountryTie, type Source } from '../lib/api.ts'
  import { QUICK, countryName, searchCountries } from '../lib/countries.ts'
  import { checkCountries, newTie, setTaxHome } from '../lib/wizard.ts'
  import { toast } from '../lib/toast.svelte.ts'

  let { source, saveLabel, onsaved }: { source: Source; saveLabel: string; onsaved: () => void } = $props()

  let list = $state<CountryTie[]>([])
  let query = $state('')
  let error = $state('')
  let busy = $state(false)
  const results = $derived(searchCountries(query).filter((c) => !list.some((x) => x.code === c.code)))

  call('profile_get')
    .then((v) => (list = v.profile.countries))
    .catch(() => {})

  function add(code: string) {
    if (list.some((c) => c.code === code)) return
    list = [...list, newTie(code, list.length === 0)]
    query = ''
    error = ''
  }
  function remove(code: string) {
    list = list.filter((c) => c.code !== code)
  }
  function toggleQuick(code: string) {
    if (list.some((c) => c.code === code)) remove(code)
    else add(code)
  }
  function flagChange(code: string, key: 'residence' | 'citizen' | 'holdsMoney', on: boolean) {
    list = list.map((c) => (c.code === code ? { ...c, [key]: on } : c))
  }

  async function save() {
    error = checkCountries(list) ?? ''
    if (error) return
    busy = true
    try {
      await call('set_countries', { countries: $state.snapshot(list), source })
      onsaved()
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }
</script>

<div class="stack">
  <div class="field">
    <span class="label" id="quick-label">Quick picks</span>
    <div class="row-actions" role="group" aria-labelledby="quick-label">
      {#each QUICK as code (code)}
        <button class="chip" aria-pressed={list.some((c) => c.code === code)} onclick={() => toggleQuick(code)}>{countryName(code)}</button>
      {/each}
    </div>
  </div>

  <div class="field search">
    <label for="country-search">Find any country</label>
    <input id="country-search" class="input" type="search" placeholder="Start typing, for example Singapore" bind:value={query} autocomplete="off" aria-controls="country-results" />
    {#if results.length}
      <ul id="country-results" class="results" aria-label="Matching countries">
        {#each results as c (c.code)}
          <li><button class="result" onclick={() => add(c.code)}>{c.name} <span class="muted">{c.code}</span></button></li>
        {/each}
      </ul>
    {:else if query.trim()}
      <p class="help" id="country-results">No country matches “{query.trim()}”.</p>
    {/if}
  </div>

  {#each list as c (c.code)}
    <fieldset class="tie">
      <legend>{countryName(c.code)}</legend>
      <button class="remove" aria-label={`Remove ${countryName(c.code)}`} onclick={() => remove(c.code)}><X size={16} /></button>
      <div class="flags">
        <label class="check"><input type="checkbox" checked={c.residence} onchange={(e) => flagChange(c.code, 'residence', e.currentTarget.checked)} /> I live here</label>
        <label class="check"><input type="checkbox" checked={c.taxHome} onchange={(e) => (list = setTaxHome(list, c.code, e.currentTarget.checked))} /> I mainly pay tax here</label>
        <label class="check"><input type="checkbox" checked={c.citizen} onchange={(e) => flagChange(c.code, 'citizen', e.currentTarget.checked)} /> I am a citizen</label>
        <label class="check"><input type="checkbox" checked={c.holdsMoney} onchange={(e) => flagChange(c.code, 'holdsMoney', e.currentTarget.checked)} /> I hold money here</label>
      </div>
    </fieldset>
  {/each}

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="row-actions">
    <Button onclick={save} disabled={busy}>{saveLabel}</Button>
  </div>
</div>

<style>
  .chip {
    border: 1px solid var(--c-line); background: var(--c-panel); border-radius: 999px; padding: 7px 14px;
    font-weight: 600; font-size: var(--text-small); cursor: pointer; color: var(--c-ink);
  }
  .chip[aria-pressed='true'] { background: var(--c-accent); color: var(--c-on-accent); border-color: var(--c-accent); }
  .search { max-width: 420px; }
  .results { list-style: none; border: 1px solid var(--c-line); border-radius: var(--r-sm); background: var(--c-panel); overflow: hidden; }
  .result { width: 100%; text-align: left; padding: 8px 12px; border: 0; background: none; cursor: pointer; display: flex; justify-content: space-between; }
  .result:hover, .result:focus-visible { background: var(--c-panel-2); }
  .tie { position: relative; border: 1px solid var(--c-line); border-radius: var(--r-sm); padding: var(--space-md) var(--space-lg); background: var(--c-panel); }
  .tie legend { padding: 0 6px; margin-left: -6px; }
  .remove { position: absolute; top: 8px; right: 8px; border: 0; background: none; color: var(--c-muted); cursor: pointer; padding: 6px; border-radius: var(--r-sm); }
  .remove:hover { color: var(--c-danger); }
  .flags { display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr)); gap: 0 var(--space-lg); }
</style>

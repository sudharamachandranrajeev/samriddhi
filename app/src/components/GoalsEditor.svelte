<script lang="ts">
  // Up to three goals, most important first, each with an optional target.
  import { Button } from '@samriddhi/ui'
  import { call, type Source } from '../lib/api.ts'
  import { currencyFor, toInput } from '../lib/money.ts'
  import { GOALS, MAX_GOALS, buildGoals, toggleGoal, type GoalDraft } from '../lib/wizard.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import MoneyField from './MoneyField.svelte'

  let { source, saveLabel, onsaved }: { source: Source; saveLabel: string; onsaved: () => void } = $props()

  let picked = $state<GoalDraft[]>([])
  let currency = $state('GBP')
  let error = $state('')
  let busy = $state(false)
  const today = new Date().toISOString().slice(0, 10)

  call('profile_get')
    .then(({ profile: p }) => {
      currency = p.cash?.currency ?? currencyFor(p.countries.find((c) => c.taxHome)?.code)
      picked = p.goals.map((g) => ({ kind: g.kind, target: toInput(g.target), targetDate: g.targetDate ?? '' }))
    })
    .catch(() => {})

  async function save() {
    const built = buildGoals(picked, currency, today)
    if ('error' in built) {
      error = built.error
      return
    }
    error = ''
    busy = true
    try {
      await call('set_goals', { goals: built.goals, source })
      onsaved()
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }
</script>

<div class="stack">
  <p class="muted small" aria-live="polite">{picked.length} of {MAX_GOALS} picked{picked.length >= MAX_GOALS ? ' — remove one to pick another' : ''}.</p>
  <div class="goals" role="group" aria-label="Goals">
    {#each GOALS as g (g.kind)}
      {@const rank = picked.findIndex((p) => p.kind === g.kind)}
      <button
        class="goal"
        aria-pressed={rank >= 0}
        disabled={rank < 0 && picked.length >= MAX_GOALS}
        onclick={() => (picked = toggleGoal(picked, g.kind))}
      >
        <span class="title">{#if rank >= 0}<span class="rank" aria-label={`priority ${rank + 1}`}>{rank + 1}</span>{/if}{g.label}</span>
        <span class="muted small">{g.hint}</span>
      </button>
    {/each}
  </div>

  {#each picked as g, i (g.kind)}
    {@const label = GOALS.find((x) => x.kind === g.kind)?.label}
    <fieldset class="target">
      <legend>{i + 1}. {label}: target (optional)</legend>
      <div class="pair">
        <MoneyField id={`goal-${g.kind}-amount`} label="Amount" currency={currency} bind:value={picked[i].target} />
        <div class="field">
          <label for={`goal-${g.kind}-date`}>By when</label>
          <input id={`goal-${g.kind}-date`} class="input" type="date" min={today} bind:value={picked[i].targetDate} />
        </div>
      </div>
    </fieldset>
  {/each}

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="row-actions">
    <Button onclick={save} disabled={busy}>{saveLabel}</Button>
  </div>
</div>

<style>
  .goals { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: var(--space-sm); }
  .goal {
    text-align: left; display: flex; flex-direction: column; gap: 2px; cursor: pointer;
    padding: 12px 14px; border-radius: var(--r-sm); border: 1px solid var(--c-line); background: var(--c-panel); color: var(--c-ink);
    transition: border-color var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
  }
  .goal:hover:not(:disabled) { border-color: var(--c-muted); }
  .goal[aria-pressed='true'] { border-color: var(--c-accent); background: var(--c-accent-soft); }
  .goal:disabled { opacity: .55; cursor: not-allowed; }
  .title { font-weight: 600; display: flex; align-items: center; gap: 8px; }
  .rank { display: inline-grid; place-items: center; width: 20px; height: 20px; border-radius: 50%; background: var(--c-accent); color: var(--c-on-accent); font-size: 11px; }
  .target { border-top: 1px solid var(--c-line); padding-top: var(--space-md); }
  .pair { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: var(--space-md); max-width: 560px; }
</style>

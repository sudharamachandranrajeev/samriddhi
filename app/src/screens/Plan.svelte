<script lang="ts">
  // Plan: a target mix of asset classes for each goal, from your risk level
  // and how soon you need the money, blended into one overall target. Tier
  // mode (1-7) gives every goal the same mix instead, and says what that
  // costs. A model on illustrative figures, never advice.
  import { Card, Explain, Pill, Stat } from '@samriddhi/ui'
  import { call, type PlanView } from '../lib/api.ts'
  import { HINTS } from '../lib/hints.ts'
  import { formatMoney } from '../lib/money.ts'
  import { CLASSES, goalName, growthShare, horizonText, mixText, pct, tierText } from '../lib/portfolio.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import Disclosure from '../components/Disclosure.svelte'
  import Hint from '../components/Hint.svelte'
  import MixBar from '../components/MixBar.svelte'

  let view = $state<PlanView | null>(null)
  let busy = $state(false)

  async function load() {
    try {
      view = await call('plan_view')
    } catch (e) {
      toast.error(e)
    }
  }
  load()

  async function setTier(tier: number | null) {
    busy = true
    try {
      await call('put_setting', { key: 'plan.tier', value: tier })
      toast.show(tier === null ? 'Back to a mix per goal.' : `Tier ${tier} for every goal.`)
      await load()
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }

  const plan = $derived(view?.plan ?? null)
  const tiers = $derived(view ? Array.from({ length: view.tierCount }, (_, i) => i + 1) : [])
  const assumedYears = (name: string, years: number) => `${goalName(name)} has no date, so the plan assumes ${horizonText(years).replace('now', 'you may need it now')}, the usual for this kind of goal. Add a date in Settings, Your answers, Goals to make it yours.`
</script>

<header class="page-head">
  <div>
    <h1 tabindex="-1">Plan</h1>
    <p>A target mix for each goal, from your risk level and how soon you need the money, blended into one mix for everything you hold.</p>
  </div>
</header>

{#if view}<Disclosure text={view.disclosure} />{/if}

<div class="grid plan">
  <Card title="Your overall target" explain="The mixes for your goals, blended into one: goals with a target amount count in proportion to it, otherwise each goal counts the same. Rebalancing on the Insights screen compares your holdings with this.">
    {#if !view}
      <p class="muted">Loading…</p>
    {:else if !plan}
      <p>Answer the risk questions first, so the plan knows how much risk suits you. <a href="#settings/answers">Answer them in Settings</a>, or choose a tier below.</p>
    {:else}
      <MixBar weights={plan.blended} label="Overall target mix" />
      <div class="stats">
        <Stat label="Growth assets" value={pct(growthShare(plan.blended))} explain="Equities, property and gold together: the part that can grow most and fall most. The rest is bonds and cash." />
        <Stat label="Risk level" value={view.riskLevel ? `${view.riskLevel} of 5` : 'Not answered'} note={view.riskLabel ?? ''} explain="The lower of how much risk you are willing to take and how much loss you could absorb, from your answers. The plan turns it into a score from 0 to 100." />
        <Stat label="Goals planned" value={String(plan.perGoal.length)} explain="Each of your goals gets its own mix. With no goals, one 20-year general investing goal is assumed." />
      </div>
    {/if}
  </Card>

  <Card title="Portfolio style" explain="Per goal is the default: each goal glides to safer assets as its date nears. A tier from 1 (cautious) to 7 (adventurous) gives every goal one fixed mix instead.">
    {#if view}
      <fieldset class="tiers" disabled={busy}>
        <legend class="sr-only">Portfolio style</legend>
        <label class="chip"><input type="radio" name="tier" checked={view.tier === null} onchange={() => setTier(null)} /> Per goal</label>
        {#each tiers as t (t)}
          <label class="chip"><input type="radio" name="tier" aria-label={`Tier ${t}`} checked={view.tier === t} onchange={() => setTier(t)} /> <span aria-hidden="true">{t}</span></label>
        {/each}
      </fieldset>
      <p class="small">{tierText(view.tier, view.tierCount)}</p>
      {#if view.tier !== null}
        <p class="note-box warn small" role="status">Tier mode does not de-risk short goals: every goal gets this same mix, so money you need soon could fall just before you need it.</p>
      {/if}
    {/if}
  </Card>
</div>

{#if plan && view}
  <h2 class="section">By goal</h2>
  <div class="grid plan">
    {#each plan.perGoal as g, i (i)}
      <Card title={goalName(g.goal.name)}>
        <div class="stack">
          <p class="meta">
            <span>Needed {horizonText(g.goal.horizonYears)}{#if g.goal.target}{` · ${formatMoney(g.goal.target)}`}{/if}</span>
            {#if g.goal.horizonAssumed}
              <Pill tone="warn">Assumed horizon</Pill>
              <Explain title="Assumed horizon" text={assumedYears(g.goal.name, g.goal.horizonYears)} />
            {/if}
          </p>
          <MixBar weights={g.allocation.weights} label={`Target mix for ${goalName(g.goal.name)}`} />
          <div class="stats">
            <Stat label="Expected return" value={pct(g.allocation.expectedReturn, 1)} unit="a year" explain="The average yearly return this mix would have on the illustrative long-run figures below. Real years swing well above and below it." />
            <Stat label="Volatility" value={pct(g.allocation.volatility, 1)} unit="a year" explain="How much a typical year's return differs from the average. About two years in three land within this distance of it." />
            <Stat
              label="Growth cap"
              value={pct(view.caps[i] ?? 0)}
              note={plan.tier !== null ? 'not applied' : ''}
              tone={plan.tier !== null ? 'warn' : 'accent'}
              explain="The most this goal may hold in growth assets because of how soon it is needed: 10% within 2 years, 40% within 5, 70% within 10, 90% after that. Tier mode does not apply it."
            />
          </div>
          <p class="sr-only">{mixText(g.allocation.weights)}</p>
        </div>
      </Card>
    {/each}
  </div>

  <Card title="The figures the plan runs on" class="assumptions" explain="Fixed, made-up long-run figures for each kind of asset, so the optimiser has something to work with. They are not market data and not a forecast.">
    <table class="counts">
      <caption class="sr-only">Illustrative assumptions per asset class</caption>
      <thead><tr><th scope="col">Asset</th><th scope="col" class="n">Return a year</th><th scope="col" class="n">Volatility</th></tr></thead>
      <tbody>
        {#each view.assumptions as a (a.class)}
          <tr><td>{CLASSES.find((c) => c.id === a.class)?.label}</td><td class="n">{pct(a.expectedReturn, 1)}</td><td class="n">{pct(a.volatility, 1)}</td></tr>
        {/each}
      </tbody>
    </table>
    <p class="muted small src">{plan.assumptionsSource} Set {plan.assumptionsAsOf}.</p>
  </Card>
{/if}

<Hint {...HINTS.plan} />

<style>
  .plan { margin-top: var(--space-lg); grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); }
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(130px, 1fr)); gap: var(--space-sm); margin-top: var(--space-md); }
  .section { font-size: var(--text-title); margin-top: var(--space-xl); }
  .meta { display: flex; align-items: center; gap: var(--space-sm); flex-wrap: wrap; font-size: var(--text-small); color: var(--c-muted); }
  .tiers { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: var(--space-sm); }
  .chip {
    display: inline-flex; align-items: center; gap: 6px; padding: 6px 12px; cursor: pointer; font-weight: 600; font-size: var(--text-small);
    border: 1px solid var(--c-line); border-radius: 999px; background: var(--c-panel);
  }
  .chip:has(input:checked) { border-color: var(--c-accent); background: var(--c-accent-soft); color: var(--c-accent); }
  .chip:has(input:focus-visible) { outline: 2px solid var(--c-focus); outline-offset: 2px; }
  .chip input { accent-color: var(--c-accent); margin: 0; }
  :global(.assumptions) { margin-top: var(--space-lg); }
  .src { margin-top: var(--space-md); line-height: 1.5; }
</style>

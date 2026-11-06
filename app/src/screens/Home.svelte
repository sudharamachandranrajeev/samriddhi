<script lang="ts">
  // Home: a rough net worth from the wizard's answers, the next best step
  // (the rules' steps and the agents' suggestions in one feed), what we know
  // about you, and a learning hint. Every figure has a "?".
  import { Button, Card, Explain, Stat } from '@samriddhi/ui'
  import { call, type CountryTie, type Dismissal, type FeedView, type ProfileView } from '../lib/api.ts'
  import { countryName } from '../lib/countries.ts'
  import { homeHint } from '../lib/hints.ts'
  import { formatMoney, monthsCovered, sumMoney } from '../lib/money.ts'
  import { GOALS } from '../lib/wizard.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import Hint from '../components/Hint.svelte'
  import StepCard from '../components/StepCard.svelte'

  let { onanswer }: { onanswer: (fact: string) => void } = $props()

  let view = $state<ProfileView | null>(null)
  let steps = $state<FeedView | null>(null)
  let hidden = $state<Dismissal[]>([])
  let titles = $state<Record<string, string>>({})
  let showHidden = $state(false)

  async function load() {
    try {
      ;[view, steps, hidden] = await Promise.all([call('profile_get'), call('feed_list'), call('steps_dismissed')])
      for (const s of steps.steps) titles[s.id] = s.title
    } catch (e) {
      toast.error(e)
    }
  }
  load()

  async function restore(id: string) {
    try {
      await call('step_restore', { id })
      toast.show('Shown again.')
      await load()
    } catch (e) {
      toast.error(e)
    }
  }

  const p = $derived(view?.profile)
  const debtTotal = $derived(p?.debts ? (sumMoney(p.debts.map((d) => d.balance)) ?? null) : null)
  const months = $derived(p ? monthsCovered(p.cash, p.costs) : null)
  const hour = new Date().getHours()
  const greeting = hour < 12 ? 'Good morning' : hour < 18 ? 'Good afternoon' : 'Good evening'
  const hint = homeHint(new Date().toISOString().slice(0, 10))
  const show = (m: { minor: number; currency: string } | null | undefined) => (m ? formatMoney(m) : 'Not given')
  const ROLE: [keyof CountryTie, string][] = [
    ['residence', 'live'], ['taxHome', 'tax home'], ['citizen', 'citizen'], ['holdsMoney', 'money'],
  ]
  const label = (id: string) =>
    titles[id] ?? (id.startsWith('ask:') ? 'A question about your money' : id.startsWith('isa-') ? 'Use your ISA allowance' : id === 'cushion' ? 'Build a safety cushion' : id === 'high-interest-debt' ? 'Pay down expensive debt' : 'A step you hid')
</script>

<header class="page-head">
  <div>
    <h1 tabindex="-1">{greeting}</h1>
    <p>Here is your picture so far, and the one step that would help most.</p>
  </div>
</header>

{#if steps?.reviewDue}
  <p class="note-box warn" role="status">Some rules are past their review date, so a figure may be out of date. Check the source before acting.</p>
{/if}
{#if steps?.errors.length}
  <p class="note-box warn" role="status">Some checks could not run this time ({steps.errors.length}), so a suggestion may be missing. The rest stand.</p>
{/if}

<div class="grid home">
  <Card title="Net worth" explain="What you own minus what you owe. For now it is only your cash savings minus your debts, from your answers in set-up, so homes, pensions and investments are not in it yet. Holdings you enter under Wealth are added up there.">
    {#if view?.roughNetWorth}
      <div class="big-figure" aria-describedby="nw-note">{formatMoney(view.roughNetWorth)}</div>
    {:else}
      <div class="big-figure muted" aria-describedby="nw-note">—</div>
    {/if}
    <p id="nw-note" class="muted small rough">Rough, from your answers{view && !view.roughNetWorth ? ': add your cash and debts to see it' : ''}.</p>
    <div class="stats">
      <Stat label="Cash savings" value={show(p?.cash)} explain="Money you can reach within days: current and easy-access savings accounts. It is your cushion if pay stops or a big bill arrives." />
      <Stat label="Debts" value={p?.debts ? (debtTotal ? formatMoney(debtTotal) : 'None') : 'Not given'} tone="warn" explain="Everything you owe except a mortgage: cards, loans, overdrafts. Expensive debt usually comes before saving or investing." />
      <Stat label="Take-home a month" value={show(p?.takeHome)} explain="Your pay after tax and deductions, roughly. With your costs it shows what you could put aside." />
      <Stat label="Costs a month" value={show(p?.costs)} explain="Rent or mortgage, bills, food, travel and the rest, roughly. It sets the size of the cushion you need." />
      <Stat
        label="Cushion, in months"
        value={months ?? '—'}
        tone={months !== null && Number(months) < 3 ? 'warn' : 'ok'}
        note={months !== null && Number(months) < 3 ? 'below 3' : ''}
        explain="How many months your cash would cover your costs if income stopped. Three to six months is the usual aim."
      />
    </div>
  </Card>

  <Card title="Next best step" explain="One step at a time, from simple dated rules (a missing cushion, costly debt, a tax-free allowance about to reset, a fact we still need) and from Samriddhi's checks on your plan and holdings. Each shows its risk, effort, source and who suggested it. Nothing here acts for you.">
    {#if !steps}
      <p class="muted">Loading…</p>
    {:else if steps.steps.length}
      <StepCard step={steps.steps[0]} onchange={load} {onanswer} />
      {#if steps.steps.length > 1}
        <details class="more">
          <summary>{steps.steps.length - 1} more step{steps.steps.length > 2 ? 's' : ''}</summary>
          <div class="stack">
            {#each steps.steps.slice(1) as s (s.id)}<StepCard step={s} onchange={load} {onanswer} />{/each}
          </div>
        </details>
      {/if}
    {:else}
      <p>Nothing stands out right now. Add more about your money in Settings and new steps will appear.</p>
    {/if}
    {#if hidden.length}
      <div class="hidden">
        <button class="link-button" aria-expanded={showHidden} onclick={() => (showHidden = !showHidden)}>{showHidden ? 'Hide' : 'Show'} {hidden.length} hidden step{hidden.length > 1 ? 's' : ''}</button>
        {#if showHidden}
          <ul>
            {#each hidden as h (h.stepId)}
              <li>
                <span>{label(h.stepId)} <span class="muted small">({h.kind === 'later' ? `later, from ${h.on}` : 'not for me'}{h.reason ? `: “${h.reason}”` : ''})</span></span>
                <Button variant="ghost" onclick={() => restore(h.stepId)}>Show again</Button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}
  </Card>

  <Card title="What we know" explain="Everything here came from your answers. Change any of it in Settings, under Your answers; nothing leaves this device.">
    <div class="stack known">
      <div>
        <span class="k">Countries <Explain title="Countries" text="Where you live, pay tax, are a citizen and hold money. Tax rules and allowances depend on these." /></span>
        {#if p?.countries.length}
          <ul class="chips">
            {#each p.countries as c (c.code)}
              <li><strong>{countryName(c.code)}</strong> <span class="muted small">{ROLE.filter(([k]) => c[k]).map(([, w]) => w).join(' · ')}</span></li>
            {/each}
          </ul>
        {:else}<p class="muted small">Not given yet.</p>{/if}
      </div>
      <div>
        <span class="k">Goals <Explain title="Goals" text="Up to three, most important first. Steps that help them are shown first." /></span>
        {#if p?.goals.length}
          <ol class="goals">
            {#each p.goals as g (g.kind)}
              <li>{GOALS.find((x) => x.kind === g.kind)?.label}{#if g.target}<span class="muted small">{` · ${formatMoney(g.target)}`}</span>{/if}{#if g.targetDate}<span class="muted small">{` by ${g.targetDate}`}</span>{/if}</li>
            {/each}
          </ol>
        {:else}<p class="muted small">Not given yet.</p>{/if}
      </div>
      <div>
        <span class="k">Risk level <Explain title="Risk level" text="The lower of how much risk you are willing to take and how much loss you could absorb. Suggestions never go above it." /></span>
        <p>{p?.risk ? `${p.risk.label} (${p.risk.combined} of 5)` : 'Not answered yet.'}</p>
      </div>
    </div>
  </Card>
</div>

<Hint {...hint} />

<style>
  .home { grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); }
  .rough { margin: 6px 0 var(--space-lg); }
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: var(--space-sm); }
  .more { margin-top: var(--space-lg); }
  .more summary { cursor: pointer; font-weight: 600; color: var(--c-accent); margin-bottom: var(--space-md); }
  .hidden { margin-top: var(--space-lg); border-top: 1px solid var(--c-line); padding-top: var(--space-md); }
  .hidden ul { list-style: none; display: flex; flex-direction: column; gap: var(--space-sm); margin-top: var(--space-sm); }
  .hidden li { display: flex; gap: var(--space-md); align-items: center; justify-content: space-between; }
  .k { display: flex; align-items: center; gap: 6px; color: var(--c-muted); font-size: var(--text-caption); font-weight: 600; text-transform: uppercase; letter-spacing: .06em; margin-bottom: 6px; }
  .chips { list-style: none; display: flex; flex-direction: column; gap: 4px; }
  .goals { padding-left: 1.2em; display: flex; flex-direction: column; gap: 4px; }
</style>

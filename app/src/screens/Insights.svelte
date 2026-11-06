<script lang="ts">
  // Insights: how your holdings sit against the plan (rebalancing, with a
  // cash flow worked out on request), and three simple models on made-up
  // data: a return forecast with its band and track record, a backtest of
  // rebalancing rules, and a direct-indexing simulator. All labelled as
  // models; nothing here trades or moves money.
  import { Button, Card, Explain, Pill, Segmented, Sparkline, Stat } from '@samriddhi/ui'
  import { call, type AssetClass, type InsightsView, type RebalanceView } from '../lib/api.ts'
  import { HINTS } from '../lib/hints.ts'
  import { formatMoney, parseAmount, wholeUnits, type Money } from '../lib/money.ts'
  import { CLASSES, classLabel, mixText, pct, points } from '../lib/portfolio.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import Disclosure from '../components/Disclosure.svelte'
  import Hint from '../components/Hint.svelte'
  import LineChart from '../components/LineChart.svelte'
  import MoneyField from '../components/MoneyField.svelte'

  let reb = $state<RebalanceView | null>(null)
  let ins = $state<InsightsView | null>(null)
  let flowKind = $state<'add' | 'take'>('add')
  let flowText = $state('')
  let flowError = $state('')

  async function load(cashFlow: Money | null = null) {
    try {
      ;[reb, ins] = await Promise.all([call('rebalance_view', { cashFlow }), ins ? Promise.resolve(ins) : call('insights_view')])
    } catch (e) {
      toast.error(e)
    }
  }
  load()

  async function workOut(e: SubmitEvent) {
    e.preventDefault()
    if (!reb) return
    const parsed = parseAmount(flowText, reb.base)
    if (!parsed.ok) {
      flowError = flowText.trim() ? parsed.error : 'Type an amount first.'
      return
    }
    flowError = ''
    const m = parsed.money
    await load({ minor: flowKind === 'add' ? m.minor : -m.minor, currency: m.currency })
  }

  const moves = (flows: Record<AssetClass, Money>) =>
    CLASSES.map((c) => ({ c, m: flows[c.id] })).filter((x) => x.m && x.m.minor !== 0)
  const abs = (m: Money): Money => ({ minor: Math.abs(m.minor), currency: m.currency })
  const TONES = ['muted', 'accent', 'accent-2'] as const
  const major = (m: Money) => m.minor
  const di = $derived(ins?.directIndexing)
</script>

<header class="page-head">
  <div>
    <h1 tabindex="-1">Insights</h1>
    <p>How your holdings sit against the plan, and three simple models on made-up data that show how rebalancing, forecasting and tax-loss harvesting behave.</p>
  </div>
</header>

{#if ins}<Disclosure text={ins.disclosure} />{/if}

<div class="grid insights">
  <Card title="Rebalancing" explain="How far each kind of asset has drifted from the plan's target. A class more than 3 percentage points from its target is outside its band. These are suggestions to consider; Samriddhi never places orders.">
    {#if !reb}
      <p class="muted">Loading…</p>
    {:else if !reb.hasPlan}
      <p>There is no target yet. <a href="#settings/answers">Answer the risk questions</a> or choose a tier on the <a href="#plan">Plan</a> screen.</p>
    {:else}
      <div class="stack">
        <p class="status">
          {#if reb.total.minor <= 0}
            <span class="muted">Add holdings on the <a href="#wealth">Wealth</a> screen to see drift.</span>
          {:else}
            <Pill tone={reb.breached ? 'warn' : 'ok'}>{reb.breached ? 'Outside bands' : 'Within bands'}</Pill>
            <span class="small muted">Bands ±{pct(reb.bands.over)} per class, on {formatMoney(reb.total)} counted.</span>
          {/if}
        </p>
        <table class="counts">
          <caption class="sr-only">Target and current share per asset class</caption>
          <thead>
            <tr>
              <th scope="col">Asset</th>
              <th scope="col" class="n">Target <Explain title="Target" text="The plan's blended share for this kind of asset." /></th>
              <th scope="col" class="n">Now <Explain title="Now" text="Its share of your counted holdings today." /></th>
              <th scope="col" class="n">Drift <Explain title="Drift" text="Now minus target, in percentage points. Plus means you hold more than the target." /></th>
            </tr>
          </thead>
          <tbody>
            {#each reb.rows as r (r.assetClass)}
              <tr class:off={reb.total.minor > 0 && Math.abs(r.drift) > reb.bands.over + 1e-9}>
                <td>{classLabel(r.assetClass)}<small class="muted block num">{formatMoney(r.value)}</small></td>
                <td class="n">{pct(r.target)}</td>
                <td class="n">{pct(r.current)}</td>
                <td class="n">{reb.total.minor > 0 ? points(r.drift) : '—'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if reb.leftOut}
          <p class="note-box warn small">Not counted, no exchange rates yet: {reb.leftOut} holding{reb.leftOut === 1 ? '' : 's'} in {reb.leftOutCurrencies.join(', ')}.</p>
        {/if}

        <div class="flow">
          <p class="small"><strong>Rebalance with new money</strong> <Explain title="Cash-flow rebalancing" text="New money goes to the classes furthest below target first; money taken out comes from those furthest above. Nothing is sold for a contribution, so there is no tax or trading cost. It is only worked out here." /></p>
          <Segmented label="Cash flow" options={[{ value: 'add', label: 'Add money' }, { value: 'take', label: 'Take money out' }]} value={flowKind} onchange={(v) => (flowKind = v)} />
          <form class="stack" onsubmit={workOut} aria-label="Work out a cash flow">
          <MoneyField id="flow-amount" label={flowKind === 'add' ? 'Amount to add' : 'Amount to take out'} currency={reb.base} bind:value={flowText} help="Only worked out here; nothing is moved" />
          {#if flowError}<p class="error" role="alert">{flowError}</p>{/if}
          <div><Button variant="quiet" type="submit">Work it out</Button></div>
          </form>
        </div>

        {#if reb.cashFlow}
          <div class="result" aria-live="polite">
            <p class="small"><strong>{flowKind === 'add' ? 'Put the new money here:' : 'Take the money from here:'}</strong></p>
            <ul class="moves">
              {#each moves(reb.cashFlow.flows) as x (x.c.id)}<li><span>{x.c.label}</span><span class="num">{formatMoney(abs(x.m))}</span></li>{/each}
            </ul>
            <p class="small {reb.cashFlow.stillBreached ? 'warn-text' : 'ok-text'}">{reb.cashFlow.stillBreached ? 'Still outside the bands after this.' : 'Back within the bands after this.'}</p>
          </div>
        {/if}

        {#if reb.trades}
          <div class="result">
            <p class="small"><strong>Sell-based option, for you to consider</strong> <Explain title="Sell-based option" text="The trades that would bring every class back to target exactly, netting to zero. Selling can cost tax and fees, so new money first is usually cheaper. Samriddhi never trades." /></p>
            <ul class="moves">
              {#each moves(reb.trades) as x (x.c.id)}<li><span>{x.m.minor > 0 ? 'Buy' : 'Sell'} {x.c.label}</span><span class="num">{formatMoney(abs(x.m))}</span></li>{/each}
            </ul>
          </div>
        {/if}
      </div>
    {/if}
  </Card>

  <Card title="Next-year return forecast" explain="A simple statistical model (exponential smoothing) fitted to 20 years of made-up yearly returns per class. The band is where it expects the next year to land 8 times in 10; the track record says how often that held in the past.">
    {#if ins}
      <div class="stack">
        <p class="label-synth small"><Pill tone="warn">Synthetic</Pill> {ins.sampleLabel}</p>
        {#each ins.forecasts as f (f.assetClass)}
          {@const fc = f.forecast}
          <div class="fc">
            <div class="fc-head">
              <strong>{classLabel(f.assetClass)}</strong>
              <span class="num">{pct(fc.mean[0], 1)}</span>
              <Explain title={`${classLabel(f.assetClass)} forecast`} text={`Smoothed next-year return ${pct(fc.mean[0], 1)}, with a ${pct(fc.confidence)} band from ${pct(fc.lower[0], 1)} to ${pct(fc.upper[0], 1)}. The band held in ${fc.track.hits} of ${fc.track.trials} past years.`} />
            </div>
            <Sparkline values={f.history} range={[fc.lower[0], fc.upper[0]]} label={`${classLabel(f.assetClass)}: 20 made-up yearly returns, with next year's ${pct(fc.confidence)} band shaded`} />
            <p class="small muted">{pct(fc.confidence)} band {pct(fc.lower[0], 1)} to {pct(fc.upper[0], 1)} · band held {fc.track.hits} of {fc.track.trials} past years</p>
          </div>
        {/each}
        <p class="small muted">A guide to uncertainty, not a prediction you can rely on. It does not change your target.</p>
      </div>
    {:else}<p class="muted">Loading…</p>{/if}
  </Card>

  <Card title="Backtest of rebalancing rules" explain="The same mix replayed through 20 years of made-up monthly returns, starting with the same amount, under three rules: never rebalance, once a year, or when a band is breached. Costs and tax are not modelled.">
    {#if ins}
      <div class="stack">
        <p class="label-synth small"><Pill tone="warn">Synthetic</Pill> {ins.sampleLabel}</p>
        <p class="small">Mix: {mixText(ins.backtestTarget)}{ins.targetFromPlan ? ' (your plan).' : ' (a plain 60/40 until you answer the risk questions).'} Start {formatMoney(ins.start)}.</p>
        <LineChart
          series={ins.backtests.map((b, i) => ({ label: b.label, values: b.yearly.map(major), tone: TONES[i % 3] }))}
          summary={`Value over 20 years: ${ins.backtests.map((b) => `${b.label} ends at ${formatMoney(wholeUnits(b.finalValue))}`).join('; ')}.`}
        />
        <table class="counts">
          <caption class="sr-only">Backtest results per rule</caption>
          <thead>
            <tr>
              <th scope="col">Rule</th>
              <th scope="col" class="n">Ends <Explain title="Ends" text="What the starting amount grew to after 20 made-up years." /></th>
              <th scope="col" class="n">A year <Explain title="A year" text="The steady yearly growth rate that would give the same end value (compound annual growth rate)." /></th>
              <th scope="col" class="n">Worst fall <Explain title="Worst fall" text="The largest drop from a high point to a later low, as a share of the high (maximum drawdown)." /></th>
            </tr>
          </thead>
          <tbody>
            {#each ins.backtests as b (b.rule)}
              <tr>
                <td>{b.label}<small class="muted block">{b.rebalances} rebalance{b.rebalances === 1 ? '' : 's'} · swings {pct(b.annualVol, 1)} a year</small></td>
                <td class="n">{formatMoney(wholeUnits(b.finalValue))}</td>
                <td class="n">{pct(b.cagr, 1)}</td>
                <td class="n">{pct(b.maxDrawdown, 1)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        <p class="small muted">Costs and tax are not modelled. Past and simulated results do not predict future returns.</p>
      </div>
    {:else}<p class="muted">Loading…</p>{/if}
  </Card>

  <Card title="Direct indexing: how loss harvesting works" explain="An education-only simulator. Holding the shares of an index yourself lets you sell any that fall below what you paid, use the loss against tax on gains, and buy a similar share instead. Here on made-up shares, not a real index.">
    {#if di}
      <div class="stack">
        <p class="label-synth small"><Pill tone="warn">Illustrative</Pill> {di.label}</p>
        <p class="small">{di.stocks} made-up shares held in equal weight for {di.months / 12} years, against a copy that sells any share {pct(di.harvestThreshold)} below its cost and moves the money into a same-sector peer.</p>
        <LineChart
          series={[
            { label: 'Plain index', values: di.indexValues.map(major), tone: 'muted' },
            { label: 'With loss harvesting', values: di.portfolioValues.map(major), tone: 'accent' },
          ]}
          summary={`Over ${di.months / 12} years the plain index ends at ${formatMoney(wholeUnits(di.indexValues[di.indexValues.length - 1]))} and the harvesting copy at ${formatMoney(wholeUnits(di.portfolioValues[di.portfolioValues.length - 1]))}.`}
        />
        <div class="stats">
          <Stat label="Harvests" value={String(di.harvests)} explain="Times a share was sold below its cost and swapped for a same-sector peer." />
          <Stat label="Losses realised" value={formatMoney(wholeUnits(di.harvestedLosses))} explain="The losses those sales locked in, which could be set against gains for tax." />
          <Stat label="Tax it could offset" value={formatMoney(wholeUnits(di.taxOffsetValue))} note={`at ${pct(di.taxRate)}`} explain="The losses times an illustrative tax rate, only if you have gains to set them against. Each swap lowers the new share's cost, so much of this is tax put off, not saved." />
          <Stat label="Tracking difference" value={pct(di.trackingDifference, 1)} unit="a year" explain="How far the harvesting copy's yearly return strays from the plain index, as a standard deviation. The price of harvesting." />
        </div>
        <p class="small muted">Education only. Wash-sale rules, costs, real index weights and your own tax rules are not modelled. Not advice.</p>
      </div>
    {:else}<p class="muted">Loading…</p>{/if}
  </Card>
</div>

<Hint {...HINTS.insights} />

<style>
  .insights { margin-top: var(--space-lg); grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); }
  .status { display: flex; gap: var(--space-sm); align-items: center; flex-wrap: wrap; }
  .block { display: block; font-size: var(--text-caption); }
  tr.off td { color: var(--c-warn); }
  .flow { display: flex; flex-direction: column; gap: var(--space-sm); border-top: 1px solid var(--c-line); padding-top: var(--space-md); }
  .result { border-left: 3px solid var(--c-accent); padding-left: var(--space-md); display: flex; flex-direction: column; gap: 6px; }
  .moves { list-style: none; display: flex; flex-direction: column; gap: 2px; font-size: var(--text-small); }
  .moves li { display: flex; justify-content: space-between; gap: var(--space-md); }
  .warn-text { color: var(--c-warn); }
  .ok-text { color: var(--c-ok); }
  .label-synth { display: flex; gap: var(--space-sm); align-items: center; flex-wrap: wrap; color: var(--c-muted); }
  .fc { display: flex; flex-direction: column; gap: 4px; padding-bottom: var(--space-sm); border-bottom: 1px solid var(--c-line); }
  .fc-head { display: flex; gap: var(--space-sm); align-items: center; }
  .fc-head .num { margin-left: auto; font-weight: 600; }
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: var(--space-sm); }
</style>

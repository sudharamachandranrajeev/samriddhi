<script lang="ts">
  // Attitude and capacity shown separately, in plain words, and the level
  // suggestions keep to.
  import { Explain } from '@samriddhi/ui'
  import type { RiskProfile } from '../lib/api.ts'
  import { ATTITUDE_WORDS, CAPACITY_WORDS } from '../lib/wizard.ts'

  let { result, compact = false }: { result: RiskProfile; compact?: boolean } = $props()
</script>

{#if compact}
  <span><strong>{result.label}</strong> ({result.combined} of 5)</span>
{:else}
  <div class="result">
    <div class="two">
      <div class="box">
        <span class="k">Attitude <Explain title="Attitude to risk" text="How much ups and downs you said you are willing to live with, from 1 (very cautious) to 5 (very adventurous)." /></span>
        <span class="v">{ATTITUDE_WORDS[result.attitude]}</span>
        <span class="meter" role="img" aria-label={`${result.attitude} of 5`}>{#each [1, 2, 3, 4, 5] as n (n)}<i class:on={n <= result.attitude}></i>{/each}</span>
      </div>
      <div class="box">
        <span class="k">Capacity for loss <Explain title="Capacity for loss" text="How big a fall you could absorb without your life or plans changing. It comes from when you need the money, your cash cushion and how steady your income is." /></span>
        <span class="v">{CAPACITY_WORDS[result.capacity].split(':')[0]}</span>
        <span class="muted small">{CAPACITY_WORDS[result.capacity].split(': ')[1]}</span>
      </div>
    </div>
    <p class="combined"><strong>Suggestions will keep to: {result.label} ({result.combined} of 5).</strong> {result.summary}</p>
    <p class="muted small">You can change this any time in Settings, under Your answers.</p>
  </div>
{/if}

<style>
  .result { display: flex; flex-direction: column; gap: var(--space-md); }
  .two { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: var(--space-md); }
  .box { display: flex; flex-direction: column; gap: 4px; padding: var(--space-md) var(--space-lg); border-radius: var(--r-sm); background: var(--c-panel-2); }
  .k { display: flex; align-items: center; gap: 6px; color: var(--c-muted); font-size: var(--text-caption); font-weight: 600; text-transform: uppercase; letter-spacing: .06em; }
  .v { font-family: var(--font-heading); font-weight: var(--weight-heading); font-size: var(--text-title); }
  .combined { line-height: 1.55; }
</style>

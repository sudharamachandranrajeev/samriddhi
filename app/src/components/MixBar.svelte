<script lang="ts">
  // A mix of asset classes as one bar with a legend. The legend's words and
  // percentages are the text alternative; the bar itself is decoration.
  import type { AssetClass, ClassWeights } from '../lib/api.ts'
  import { CLASSES, mixText, pct } from '../lib/portfolio.ts'

  let { weights, label, compare }: { weights: Partial<ClassWeights>; label: string; compare?: Partial<ClassWeights> } = $props()

  const shown = $derived(CLASSES.filter((c) => (weights[c.id] ?? 0) >= 0.005 || (compare?.[c.id] ?? 0) >= 0.005))
  const w = (c: AssetClass) => Math.max(0, weights[c] ?? 0)
</script>

<figure class="mix">
  <figcaption class="sr-only">{label}: {mixText(weights)}</figcaption>
  <div class="bar" aria-hidden="true">
    {#each CLASSES as c (c.id)}
      {#if w(c.id) > 0}<span class="seg {c.id}" style:flex-grow={w(c.id)}></span>{/if}
    {/each}
  </div>
  <ul class="legend" aria-hidden="true">
    {#each shown as c (c.id)}
      <li><i class="dot {c.id}"></i>{c.label} <span class="num">{pct(w(c.id))}</span>{#if compare}<span class="muted num"> · now {pct(compare[c.id] ?? 0)}</span>{/if}</li>
    {/each}
  </ul>
</figure>

<style>
  .mix { --mix-property: #7a4fd0; --mix-gold: #a8761a; display: flex; flex-direction: column; gap: var(--space-sm); margin: 0; }
  .bar { display: flex; height: 12px; border-radius: 6px; overflow: hidden; background: var(--c-line); gap: 2px; }
  .seg { flex-basis: 0; min-width: 2px; }
  .legend { list-style: none; display: flex; flex-wrap: wrap; gap: 4px var(--space-md); font-size: var(--text-small); }
  .legend li { display: inline-flex; align-items: center; gap: 6px; }
  .dot { width: 10px; height: 10px; border-radius: 3px; display: inline-block; }
  .equity { background: var(--c-accent); }
  .bonds { background: var(--c-accent-2); }
  /* Property and gold get hues of their own (the themes' ok and warn
     colours sit too close to their accents), and gold is striped so it
     never reads as the equity colour in the gold-accented theme. */
  .property { background: var(--mix-property); }
  .gold { background: repeating-linear-gradient(135deg, var(--mix-gold) 0 3px, color-mix(in oklab, var(--mix-gold) 45%, var(--c-panel)) 3px 6px); }
  .cash { background: var(--c-muted); }
  :global([data-mode='dark']) .mix { --mix-property: #b9a0ff; --mix-gold: #f0d58c; }
</style>

<script lang="ts">
  // A small chart of a few lines on one scale. `summary` is the text
  // alternative read out instead of the drawing; the legend names each line.
  import { linePoints } from '../lib/portfolio.ts'

  type Tone = 'muted' | 'accent' | 'accent-2' | 'ok' | 'warn'
  let { series, summary, height = 140 }: { series: { label: string; values: number[]; tone: Tone }[]; summary: string; height?: number } = $props()

  const W = 300
  const all = $derived(series.flatMap((s) => s.values))
  const lo = $derived(Math.min(...all))
  const hi = $derived(Math.max(...all))
</script>

<figure class="chart">
  <svg viewBox={`0 0 ${W} ${height}`} preserveAspectRatio="none" role="img" aria-label={summary} style:height={`${height}px`}>
    {#each [0, 0.5, 1] as f (f)}<line x1="0" x2={W} y1={f * height} y2={f * height} class="grid" vector-effect="non-scaling-stroke" />{/each}
    {#each series as s (s.label)}
      <polyline points={linePoints(s.values, W, height, lo, hi)} class={s.tone} vector-effect="non-scaling-stroke" />
    {/each}
  </svg>
  <figcaption>
    <ul class="legend">
      {#each series as s (s.label)}<li><i class={`key ${s.tone}`} aria-hidden="true"></i>{s.label}</li>{/each}
    </ul>
  </figcaption>
</figure>

<style>
  .chart { margin: 0; display: flex; flex-direction: column; gap: var(--space-sm); }
  svg { width: 100%; display: block; overflow: visible; }
  .grid { stroke: var(--c-line); stroke-width: 1; }
  polyline { fill: none; stroke-width: 2; stroke-linejoin: round; stroke-linecap: round; }
  polyline.muted { stroke: var(--c-muted); stroke-dasharray: 4 3; }
  polyline.accent { stroke: var(--c-accent); }
  polyline.accent-2 { stroke: var(--c-accent-2); }
  polyline.ok { stroke: var(--c-ok); }
  polyline.warn { stroke: var(--c-warn); }
  .legend { list-style: none; display: flex; flex-wrap: wrap; gap: 4px var(--space-md); font-size: var(--text-small); }
  .legend li { display: inline-flex; align-items: center; gap: 6px; }
  .key { width: 14px; height: 3px; border-radius: 2px; display: inline-block; }
  .key.muted { background: var(--c-muted); }
  .key.accent { background: var(--c-accent); }
  .key.accent-2 { background: var(--c-accent-2); }
  .key.ok { background: var(--c-ok); }
  .key.warn { background: var(--c-warn); }
</style>

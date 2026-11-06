<script lang="ts">
  // A tiny trend line. The shaded band is the normal range when one is given (same units as values).
  let {
    values,
    range,
    tone = 'accent',
    label,
  }: { values: number[]; range?: [number, number]; tone?: 'accent' | 'ok' | 'warn' | 'danger'; label: string } = $props()

  const box = $derived.by(() => {
    const all = range ? [...values, ...range] : values
    const lo = Math.min(...all)
    const hi = Math.max(...all)
    const pad = (hi - lo || 1) * 0.15
    return { lo: lo - pad, hi: hi + pad }
  })
  const y = (v: number) => 28 - ((v - box.lo) / (box.hi - box.lo)) * 28
  const points = $derived(values.map((v, i) => `${(i / Math.max(1, values.length - 1)) * 100},${y(v).toFixed(1)}`).join(' '))
</script>

<svg viewBox="0 0 100 28" preserveAspectRatio="none" role="img" aria-label={label}>
  {#if range}<rect x="0" width="100" y={y(range[1])} height={y(range[0]) - y(range[1])} class="band" />{/if}
  <polyline {points} class={tone} vector-effect="non-scaling-stroke" />
</svg>

<style>
  svg { width: 100%; height: 28px; display: block; overflow: visible; }
  .band { fill: var(--c-accent-soft); opacity: .7; }
  polyline { fill: none; stroke-width: 2; stroke-linejoin: round; stroke-linecap: round; }
  .accent { stroke: var(--c-accent); }
  .ok { stroke: var(--c-ok); }
  .warn { stroke: var(--c-warn); }
  .danger { stroke: var(--c-danger); }
</style>

<script lang="ts">
  import Explain from './Explain.svelte'
  import Sparkline from './Sparkline.svelte'

  type Tone = 'accent' | 'ok' | 'warn' | 'danger'
  let {
    label,
    value,
    unit = '',
    note = '',
    tone = 'accent',
    explain,
    trend,
    range,
  }: {
    label: string
    value: string
    unit?: string
    note?: string
    tone?: Tone
    explain?: string
    trend?: number[]
    range?: [number, number]
  } = $props()
</script>

<div class="stat">
  <div class="top">
    <span class="label">{label}</span>
    {#if explain}<Explain title={label} text={explain} />{/if}
    {#if note}<span class="note {tone}">{note}</span>{/if}
  </div>
  <div class="value">{value}<span class="unit">{unit}</span></div>
  {#if trend}<Sparkline values={trend} {range} {tone} label={`${label} trend`} />{/if}
</div>

<style>
  .stat { background: var(--row-bg); border: var(--row-divider); border-radius: var(--r-sm); padding: 10px var(--space-md); min-width: 0; }
  .top { display: flex; align-items: center; gap: 6px; font-size: var(--text-caption); color: var(--c-muted); }
  .note { margin-left: auto; font-weight: 600; white-space: nowrap; }
  .ok { color: var(--c-ok); } .warn { color: var(--c-warn); } .danger { color: var(--c-danger); } .accent { color: var(--c-accent); }
  .value { font-family: var(--font-number); font-size: var(--text-stat); font-weight: var(--weight-display); letter-spacing: -.02em; margin: 2px 0 4px; }
  .unit { font-family: var(--font-body); font-size: 11px; color: var(--c-muted); margin-left: 4px; font-weight: 500; letter-spacing: 0; }
</style>

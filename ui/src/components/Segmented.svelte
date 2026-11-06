<script lang="ts" generics="T extends string">
  let {
    label,
    options,
    value,
    onchange,
  }: { label: string; options: { value: T; label: string }[]; value: T; onchange: (v: T) => void } = $props()
</script>

<div class="seg" role="radiogroup" aria-label={label}>
  {#each options as o (o.value)}
    <button role="radio" aria-checked={o.value === value} class:on={o.value === value} onclick={() => onchange(o.value)}>{o.label}</button>
  {/each}
</div>

<style>
  .seg { display: inline-flex; padding: 3px; gap: 2px; border-radius: var(--r-control); background: var(--c-panel-2); border: 1px solid var(--c-line); }
  button {
    border: 0; background: transparent; color: var(--c-muted); font-weight: 600; font-size: var(--text-small);
    padding: 6px 14px; border-radius: calc(var(--r-control) - 3px); cursor: pointer;
    transition: background var(--dur-base) var(--ease), color var(--dur-base) var(--ease);
  }
  button.on { background: var(--c-panel); color: var(--c-ink); box-shadow: 0 1px 3px rgb(0 0 0 / .12); }
</style>

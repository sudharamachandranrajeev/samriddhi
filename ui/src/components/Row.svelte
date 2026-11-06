<script lang="ts">
  import type { Snippet } from 'svelte'

  let { lead, title, sub, trailing }: { lead?: string; title: string; sub?: string; trailing?: Snippet } = $props()
</script>

<div class="row">
  {#if lead}<span class="lead">{lead}</span>{/if}
  <span class="what">{title}{#if sub}<small>{sub}</small>{/if}</span>
  {#if trailing}<span class="trail">{@render trailing()}</span>{/if}
</div>

<style>
  .row {
    display: flex; align-items: center; gap: var(--space-md);
    padding: 10px var(--space-md); background: var(--row-bg); border-radius: var(--row-radius);
    border-bottom: var(--row-divider); font-size: var(--text-body);
  }
  .lead { font-family: var(--font-number); font-size: var(--text-caption); color: var(--c-muted); width: 44px; flex: 0 0 auto; }
  .what { min-width: 0; }
  small { display: block; color: var(--c-muted); font-size: var(--text-caption); margin-top: 2px; }
  .trail { margin-left: auto; }
</style>

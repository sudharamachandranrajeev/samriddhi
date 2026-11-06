<script lang="ts">
  import type { Snippet } from 'svelte'

  type Tone = 'accent' | 'ok' | 'warn' | 'danger' | 'done'
  let { tone = 'accent', onclick, children }: { tone?: Tone; onclick?: () => void; children: Snippet } = $props()
</script>

{#if onclick}
  <button class="pill {tone}" {onclick}>{@render children()}</button>
{:else}
  <span class="pill {tone}">{@render children()}</span>
{/if}

<style>
  .pill {
    display: inline-flex; align-items: center; gap: var(--space-xs); white-space: nowrap;
    font-size: 11.5px; font-weight: 600; padding: 4px 10px; border-radius: 999px; border: 1px solid transparent;
    transition: transform var(--dur-fast) var(--spring), filter var(--dur-fast) var(--ease);
  }
  button.pill { cursor: pointer; }
  button.pill:hover { filter: brightness(1.05); transform: translateY(-1px); }
  button.pill:active { transform: translateY(0) scale(.97); }
  .accent { background: var(--c-accent-soft); color: var(--c-accent); }
  .ok { background: var(--c-ok-soft); color: var(--c-ok); }
  .warn { background: var(--c-warn-soft); color: var(--c-warn); }
  .danger { background: var(--c-danger-soft); color: var(--c-danger); }
  .done { background: transparent; color: var(--c-ok); border-color: currentColor; }
</style>

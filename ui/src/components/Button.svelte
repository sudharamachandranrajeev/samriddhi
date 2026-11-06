<script lang="ts">
  import type { Snippet } from 'svelte'

  let {
    variant = 'primary',
    type = 'button',
    disabled = false,
    onclick,
    children,
  }: {
    variant?: 'primary' | 'quiet' | 'ghost'
    type?: 'button' | 'submit'
    disabled?: boolean
    onclick?: (e: MouseEvent) => void
    children: Snippet
  } = $props()
</script>

<button class={variant} {type} {disabled} {onclick}>{@render children()}</button>

<style>
  button {
    display: inline-flex; align-items: center; justify-content: center; gap: var(--space-sm);
    min-height: 38px; padding: 0 var(--space-lg); border-radius: var(--r-control);
    border: 1px solid transparent; font-weight: 600; font-size: var(--text-small); cursor: pointer;
    transition: transform var(--dur-fast) var(--spring), background var(--dur-base) var(--ease), box-shadow var(--dur-base) var(--ease);
  }
  button:active:not(:disabled) { transform: scale(.97); }
  button:disabled { opacity: .5; cursor: not-allowed; }
  .primary { background: var(--c-accent); color: var(--c-on-accent); box-shadow: var(--glow); }
  .quiet { background: var(--c-accent-soft); color: var(--c-accent); }
  .ghost { background: transparent; color: var(--c-ink); border-color: var(--c-line); }
  .ghost:hover { border-color: var(--c-muted); }
</style>

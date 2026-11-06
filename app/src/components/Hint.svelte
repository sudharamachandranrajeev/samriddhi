<script lang="ts">
  // The learning hint every screen carries: one idea, plain words.
  import Lightbulb from '@lucide/svelte/icons/lightbulb'

  let { title, text }: { title: string; text: string } = $props()
</script>

<aside class="hint" aria-label="Learning hint">
  <span class="icon" aria-hidden="true"><Lightbulb size={18} /></span>
  <div>
    <strong>Learn: {title}</strong>
    <p>{text}</p>
  </div>
</aside>

<style>
  .hint {
    display: flex; gap: var(--space-md); align-items: flex-start; margin-top: var(--space-xl);
    padding: var(--space-md) var(--space-lg); border-radius: var(--r-sm);
    background: var(--c-accent-soft); color: var(--c-ink); max-width: 760px;
  }
  .icon { color: var(--c-accent); margin-top: 1px; }
  strong { display: block; font-weight: 600; margin-bottom: 2px; color: var(--c-accent); }
  p { font-size: var(--text-small); line-height: 1.5; }
</style>

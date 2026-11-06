<script lang="ts">
  // The "model, not advice" line every plan and forecast result carries.
  // The text comes from the core, so it is the same everywhere.
  import Info from '@lucide/svelte/icons/info'

  let { text }: { text: string } = $props()
</script>

<p class="disclosure note-box small" role="note">
  <span class="icon" aria-hidden="true"><Info size={16} /></span>
  <span><strong>A model, not advice.</strong> {text}</span>
</p>

<style>
  .disclosure { display: flex; gap: var(--space-sm); align-items: flex-start; max-width: 80ch; line-height: 1.5; }
  .icon { color: var(--c-accent); margin-top: 2px; flex: 0 0 auto; }
  strong { color: var(--c-ink); font-weight: 600; }
</style>

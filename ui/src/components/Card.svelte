<script lang="ts">
  import type { Snippet } from 'svelte'
  import Explain from './Explain.svelte'

  let {
    title,
    explain,
    action,
    children,
    class: cls = '',
  }: { title?: string; explain?: string; action?: Snippet; children: Snippet; class?: string } = $props()
</script>

<section class="card {cls}">
  {#if title}
    <header>
      <h2>{title}</h2>
      {#if explain}<Explain {title} text={explain} />{/if}
      {#if action}<span class="action">{@render action()}</span>{/if}
    </header>
  {/if}
  {@render children()}
</section>

<style>
  .card {
    background: var(--card-bg);
    border: var(--card-border);
    box-shadow: var(--card-shadow);
    backdrop-filter: blur(var(--card-blur));
    border-radius: var(--r);
    padding: var(--space-lg) var(--space-xl);
    position: relative;
    min-width: 0;
    transition: background var(--dur-base) var(--ease), border-radius var(--dur-base) var(--ease);
  }
  header { display: flex; align-items: center; gap: var(--space-sm); margin-bottom: var(--space-md); }
  h2 {
    font-family: var(--title-font);
    font-size: var(--title-size);
    text-transform: var(--title-case);
    letter-spacing: var(--title-tracking);
    color: var(--title-color);
    font-weight: 600;
  }
  .action { margin-left: auto; font-size: var(--text-caption); font-weight: 600; }
</style>

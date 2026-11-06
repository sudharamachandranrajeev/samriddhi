<script lang="ts">
  // App frame: side rail on wide windows, bottom tabs on phones. Same items, same order.
  import type { Snippet } from 'svelte'
  import Ellipsis from '@lucide/svelte/icons/ellipsis'
  import type { NavItem } from '../types.ts'

  let {
    brand,
    items,
    current,
    onnavigate,
    footer,
    tabs = 4,
    children,
  }: {
    brand: string
    items: NavItem[]
    current: string
    onnavigate: (id: string) => void
    footer?: Snippet
    tabs?: number
    children: Snippet
  } = $props()

  let moreOpen = $state(false)
  const tabItems = $derived(items.slice(0, tabs))
  const moreItems = $derived(items.slice(tabs))
  const go = (id: string) => { moreOpen = false; onnavigate(id) }
</script>

<div class="shell">
  <nav class="rail" aria-label="Main">
    <div class="brand"><i aria-hidden="true"></i>{brand}</div>
    {#each items as item (item.id)}
      <button class:on={item.id === current} aria-current={item.id === current ? 'page' : undefined} onclick={() => go(item.id)}>
        <span class="icon" aria-hidden="true"><item.icon size={18} /></span>{item.label}
      </button>
    {/each}
    <span class="spacer"></span>
    {#if footer}<div class="foot">{@render footer()}</div>{/if}
  </nav>

  <main>{@render children()}</main>

  <nav class="tabs" aria-label="Main">
    {#each tabItems as item (item.id)}
      <button class:on={item.id === current} aria-current={item.id === current ? 'page' : undefined} onclick={() => go(item.id)}>
        <span class="icon" aria-hidden="true"><item.icon size={18} /></span>{item.label}
      </button>
    {/each}
    {#if moreItems.length}
      <button class:on={moreItems.some((i) => i.id === current)} aria-expanded={moreOpen} onclick={() => (moreOpen = !moreOpen)}>
        <span class="icon" aria-hidden="true"><Ellipsis size={18} /></span>More
      </button>
    {/if}
  </nav>
  {#if moreOpen}
    <div class="more" role="menu">
      {#each moreItems as item (item.id)}
        <button role="menuitem" onclick={() => go(item.id)}><span class="icon" aria-hidden="true"><item.icon size={18} /></span>{item.label}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .shell { display: grid; grid-template-columns: 228px 1fr; min-height: 100vh; }
  .rail { position: sticky; top: 0; height: 100vh; padding: 26px 16px; border-right: 1px solid var(--c-line); display: flex; flex-direction: column; gap: 2px; }
  .brand { font-family: var(--font-heading); font-weight: var(--weight-heading); font-size: 19px; margin: 0 10px 24px; display: flex; gap: 10px; align-items: center; line-height: 1.15; }
  .brand i { width: 28px; height: 28px; border-radius: calc(var(--r-sm) * .75); background: linear-gradient(135deg, var(--c-accent), var(--c-accent-2)); box-shadow: var(--glow); flex: 0 0 auto; }
  .rail button {
    display: flex; gap: 10px; align-items: center; padding: 9px 12px; border: 0; background: transparent; cursor: pointer;
    border-radius: var(--nav-on-radius); color: var(--c-muted); font-weight: 500; font-size: var(--text-body); text-align: left;
    border-left: var(--nav-on-bar) solid transparent;
    transition: background var(--dur-base) var(--ease), color var(--dur-base) var(--ease);
  }
  .rail button:hover { color: var(--c-ink); }
  .rail button.on { background: var(--nav-on-bg); color: var(--nav-on-ink); border-left-color: var(--c-accent); }
  .icon { width: 20px; display: inline-flex; justify-content: center; }
  .spacer { flex: 1; }
  .foot { font-size: var(--text-caption); color: var(--c-muted); padding: 12px; border-top: 1px solid var(--c-line); }
  main { padding: 28px 34px; min-width: 0; }
  .tabs, .more { display: none; }

  @media (max-width: 860px) {
    .shell { grid-template-columns: 1fr; padding-bottom: 76px; }
    .rail { display: none; }
    main { padding: 20px 16px; }
    .tabs {
      display: flex; position: fixed; inset: auto 0 0 0; justify-content: space-around; z-index: 10;
      padding: 8px 6px calc(10px + env(safe-area-inset-bottom)); background: var(--c-panel); border-top: 1px solid var(--c-line);
    }
    .tabs button { display: flex; flex-direction: column; align-items: center; gap: 3px; border: 0; background: none; color: var(--c-muted); font-size: 11px; padding: 4px 10px; cursor: pointer; }
    .tabs button.on { color: var(--c-accent); }
    .more {
      display: flex; flex-direction: column; position: fixed; right: 10px; bottom: 80px; z-index: 11; min-width: 200px;
      background: var(--c-panel); border: 1px solid var(--c-line); border-radius: var(--r-sm); padding: 6px; box-shadow: 0 12px 32px rgb(0 0 0 / .2);
    }
    .more button { display: flex; gap: 10px; padding: 10px 12px; border: 0; background: none; text-align: left; cursor: pointer; border-radius: var(--r-sm); }
    .more button:hover { background: var(--c-panel-2); }
  }
</style>

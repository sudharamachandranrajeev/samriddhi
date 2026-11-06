<script lang="ts">
  import { toast } from '../lib/toast.svelte.ts'
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each toast.items as t (t.id)}
    <p class="t {t.tone}">{t.text}</p>
  {/each}
</div>

<style>
  .toasts { position: fixed; right: 16px; bottom: 16px; z-index: 40; display: flex; flex-direction: column; gap: 8px; max-width: min(420px, calc(100vw - 32px)); }
  .t { padding: 10px 14px; border-radius: var(--r-sm); font-weight: 600; font-size: var(--text-small); box-shadow: 0 10px 30px rgb(0 0 0 / .2); }
  .ok { background: var(--c-ok-soft); color: var(--c-ok); }
  .danger { background: var(--c-danger-soft); color: var(--c-danger); }
  @media (max-width: 860px) { .toasts { bottom: 90px; } }
</style>

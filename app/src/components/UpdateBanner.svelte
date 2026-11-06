<script lang="ts">
  // Shown at the top of every page when a new version is out. Nothing is
  // installed until the user presses the button, and "Later" always works.
  import { Button } from '@samriddhi/ui'
  import { call } from '../lib/api.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import { sizeText, updates } from '../lib/updates.svelte.ts'

  const u = $derived(updates.found)
  let notes = $state(false)
  const size = $derived(u?.size ?? updates.total)
</script>

{#if u && !updates.dismissed}
  <div class="banner" role="status">
    <p>
      <strong>Samriddhi {u.version} is out.</strong>
      {#if size}<span class="muted">({sizeText(size)})</span>{/if}
      {#if u.canInstall}
        You have {updates.status?.current}. Your data is backed up before anything is installed, and you can go back afterwards in Settings, Updates.
      {:else if updates.status?.kind === 'portable'}
        This is the portable version, so it cannot update itself. Download the new version and replace this folder's files with it. Your data stays where it is.
      {:else}
        This copy cannot update itself. Download the new version from the release page.
      {/if}
      {#if u.notes}<button class="link-button" onclick={() => (notes = !notes)}>{notes ? 'Hide what is new' : 'What is new'}</button>{/if}
    </p>
    {#if notes && u.notes}<pre class="notes">{u.notes}</pre>{/if}
    {#if updates.progress !== null}<progress max="1" value={updates.progress} aria-label="Download"></progress>{/if}
    <div class="row-actions">
      {#if u.canInstall}
        <Button disabled={updates.busy} onclick={() => updates.install()}>{updates.busy ? 'Installing…' : 'Install and restart'}</Button>
      {:else}
        <Button onclick={() => call('open_page', { page: 'releases', fields: {} }).catch((e) => toast.error(e))}>Open the download page</Button>
      {/if}
      <Button variant="ghost" disabled={updates.busy} onclick={() => (updates.dismissed = true)}>Later</Button>
    </div>
  </div>
{/if}

<style>
  .banner {
    display: flex; flex-direction: column; gap: var(--space-sm);
    margin-bottom: var(--space-lg); padding: var(--space-md) var(--space-lg); border-radius: var(--r-sm);
    background: var(--c-accent-soft); border-left: 4px solid var(--c-accent);
  }
  .banner p { margin: 0; max-width: 90ch; line-height: 1.5; }
  .notes { white-space: pre-wrap; font: inherit; font-size: var(--text-small); max-height: 200px; overflow: auto; color: var(--c-muted); margin: 0; }
  progress { width: 100%; accent-color: var(--c-accent); }
</style>

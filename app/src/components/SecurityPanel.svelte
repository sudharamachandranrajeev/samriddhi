<script lang="ts">
  // Security status: one row per protection, in plain words, with what is
  // fine, what is worth a look and what does not apply on this device. The
  // only switch is blocking screenshots, and only on phones.
  import { Card, Pill } from '@samriddhi/ui'
  import { call, type SecurityLevel, type SecurityStatus } from '../lib/api.ts'
  import { toast } from '../lib/toast.svelte.ts'

  let { mobile }: { mobile: boolean } = $props()

  let status = $state<SecurityStatus | null>(null)
  let busy = $state(false)

  async function load() {
    try {
      status = await call('security_status')
    } catch (e) {
      toast.error(e)
    }
  }
  load()

  async function setCapture(on: boolean) {
    busy = true
    try {
      await call('set_block_capture', { on })
      toast.show(on ? 'Screenshots and screen recording of Samriddhi are now blocked.' : 'Screenshots are allowed again.')
      await load()
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }

  const TONE: Record<SecurityLevel, 'ok' | 'accent' | 'warn' | 'done'> = { good: 'ok', info: 'accent', warn: 'warn', na: 'done' }
  const WORD: Record<SecurityLevel, string> = { good: 'Fine', info: 'Note', warn: 'Worth a look', na: 'Not here' }
</script>

<Card title="Security status" explain="What protects your data on this device, checked now. It only reports: nothing is sent anywhere, and nothing changes unless you use the switch.">
  {#if !status}
    <p class="muted">Loading…</p>
  {:else}
    <ul class="rows">
      {#each status.rows as r (r.id)}
        <li>
          <div class="head"><strong>{r.label}</strong><Pill tone={TONE[r.level]}>{WORD[r.level]}</Pill></div>
          <p class="small muted">{r.detail}</p>
          {#if r.id === 'captureBlock' && mobile && r.on !== undefined}
            <label class="check"><input type="checkbox" checked={r.on} disabled={busy} onchange={(e) => setCapture(e.currentTarget.checked)} /> Block screenshots and screen recording</label>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</Card>

<style>
  .rows { list-style: none; display: flex; flex-direction: column; }
  .rows li { padding: var(--space-sm) 0; border-bottom: 1px solid var(--c-line); display: flex; flex-direction: column; gap: 4px; }
  .rows li:last-child { border-bottom: 0; }
  .head { display: flex; justify-content: space-between; align-items: center; gap: var(--space-md); flex-wrap: wrap; }
</style>

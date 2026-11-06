<script lang="ts">
  // A locked app (PIN, fingerprint or face), or data this device can no
  // longer open on its own (recovery key). The recovery key always works.
  import { Button, Card } from '@samriddhi/ui'
  import LockKeyhole from '@lucide/svelte/icons/lock-keyhole'
  import { call, type Status } from '../lib/api.ts'
  import { HINTS } from '../lib/hints.ts'
  import { checkRecoveryText } from '../lib/wizard.ts'
  import Hint from '../components/Hint.svelte'

  let { status, recover, onunlocked }: { status: Status; recover: boolean; onunlocked: () => Promise<void> } = $props()

  // svelte-ignore state_referenced_locally
  let useRecovery = $state(recover || !status.hasPin)
  let pin = $state('')
  let recovery = $state('')
  let error = $state('')
  let busy = $state(false)
  // svelte-ignore state_referenced_locally
  let wait = $state(status.waitSecs)

  $effect(() => {
    if (wait <= 0) return
    const t = setInterval(() => (wait = Math.max(0, wait - 1)), 1000)
    return () => clearInterval(t)
  })

  async function tryPin(e: SubmitEvent) {
    e.preventDefault()
    if (!/^\d{4,12}$/.test(pin)) {
      error = 'A PIN is 4 to 12 digits.'
      return
    }
    busy = true
    try {
      if (await call('unlock_pin', { pin })) {
        error = ''
        await onunlocked()
      } else {
        error = 'That PIN is not right.'
        pin = ''
        const s = await call('status')
        wait = s.waitSecs
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err)
    } finally {
      busy = false
    }
  }

  async function tryRecovery(e: SubmitEvent) {
    e.preventDefault()
    error = checkRecoveryText(recovery) ?? ''
    if (error) return
    busy = true
    try {
      await call('unlock_with_recovery', { recovery: recovery.replace(/[\s-]/g, '') })
      recovery = ''
      await onunlocked()
    } catch (err) {
      error = err instanceof Error ? err.message : String(err)
    } finally {
      busy = false
    }
  }

  async function bio() {
    busy = true
    try {
      await call('unlock_biometric')
      await onunlocked()
    } catch (err) {
      error = err instanceof Error ? err.message : String(err)
    } finally {
      busy = false
    }
  }
</script>

<main class="page lock" id="main">
  <div class="inner">
    <div class="badge" aria-hidden="true"><LockKeyhole size={28} /></div>
    <h1 tabindex="-1">{recover ? 'Open your data' : 'Samriddhi is locked'}</h1>
    <p class="why"><strong>Why we ask:</strong> <span>{recover
      ? 'This device no longer holds the key that opens your data, so only your recovery key can open it.'
      : 'The lock keeps your figures private if someone else picks up this device.'}</span></p>
    {#if recover && status.problem}<p class="note-box warn" role="status">{status.problem}</p>{/if}

    <Card>
      {#if useRecovery}
        <form class="stack" onsubmit={tryRecovery}>
          <div class="field">
            <label for="recovery">Recovery key</label>
            <textarea id="recovery" class="input num" rows="3" spellcheck="false" autocomplete="off" bind:value={recovery} aria-describedby="recovery-help"></textarea>
            <span class="help" id="recovery-help">The 64 letters and digits you saved at set-up. Spaces and dashes are fine.</span>
          </div>
          {#if error}<p class="error" role="alert">{error}</p>{/if}
          <div class="row-actions">
            <Button type="submit" disabled={busy}>Open</Button>
            {#if !recover && status.hasPin}<button type="button" class="link-button" onclick={() => { useRecovery = false; error = '' }}>Use my PIN instead</button>{/if}
          </div>
        </form>
      {:else}
        <form class="stack" onsubmit={tryPin}>
          <div class="field">
            <label for="pin">PIN</label>
            <!-- svelte-ignore a11y_autofocus -->
            <input id="pin" class="input num pin" type="password" inputmode="numeric" autocomplete="current-password" maxlength="12" autofocus bind:value={pin} disabled={wait > 0} />
          </div>
          {#if wait > 0}<p class="note-box warn" role="status">Too many wrong PINs. Try again in {wait} seconds, or use your recovery key.</p>{/if}
          {#if error}<p class="error" role="alert">{error}</p>{/if}
          <div class="row-actions">
            <Button type="submit" disabled={busy || wait > 0}>Unlock</Button>
            {#if status.biometric}<Button variant="quiet" disabled={busy} onclick={bio}>Use fingerprint or face</Button>{/if}
          </div>
          <button type="button" class="link-button" onclick={() => { useRecovery = true; error = '' }}>Forgot your PIN? Use your recovery key</button>
        </form>
      {/if}
    </Card>
    <Hint {...HINTS[recover ? 'recover' : 'unlock']} />
  </div>
</main>

<style>
  .lock { min-height: 100vh; display: grid; place-items: center; padding: 24px 16px; }
  .inner { width: min(520px, 100%); }
  .badge { width: 56px; height: 56px; border-radius: 50%; display: grid; place-items: center; background: var(--c-accent-soft); color: var(--c-accent); margin-bottom: var(--space-lg); box-shadow: var(--glow); }
  h1 { font-size: var(--text-display); outline: none; }
  .pin { font-size: 22px; letter-spacing: .3em; max-width: 240px; }
  .note-box { margin-bottom: var(--space-md); }
</style>

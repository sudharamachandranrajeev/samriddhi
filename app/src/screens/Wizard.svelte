<script lang="ts">
  // First run, eight screens. Each says in one sentence why it asks.
  import { Button, Card } from '@samriddhi/ui'
  import Check from '@lucide/svelte/icons/check'
  import { call, type RiskProfile, type Status, type Step, type UpdateMode } from '../lib/api.ts'
  import { HINTS } from '../lib/hints.ts'
  import { SAFETY, SCREENS, checkPin, groupKey, nextScreen, prevScreen, resumeAt, screenIndex, type ScreenId } from '../lib/wizard.ts'
  import { focusHeading, transition } from '../lib/transition.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import Hint from '../components/Hint.svelte'
  import CountriesEditor from '../components/CountriesEditor.svelte'
  import GoalsEditor from '../components/GoalsEditor.svelte'
  import MoneyEditor from '../components/MoneyEditor.svelte'
  import RiskEditor from '../components/RiskEditor.svelte'
  import RiskResult from '../components/RiskResult.svelte'
  import StepCard from '../components/StepCard.svelte'

  let {
    status,
    resume,
    onstatus,
    onfinish,
  }: { status: Status; resume: boolean; onstatus: () => Promise<void>; onfinish: () => Promise<void> } = $props()

  // Decided once: later status changes (the store now exists) must not move the user.
  // svelte-ignore state_referenced_locally
  let screen = $state<ScreenId>(resume ? resumeAt(status.recoveryPending) : 'welcome')
  // svelte-ignore state_referenced_locally
  let accepted = $state(resume)
  let key = $state('')
  let keySaved = $state(false)
  let pin = $state('')
  let pin2 = $state('')
  let bio = $state(false)
  let error = $state('')
  let busy = $state(false)
  let risk = $state<RiskProfile | null>(null)
  let first = $state<Step | null | undefined>(undefined)

  const meta = $derived(SCREENS[screenIndex(screen)])
  const mobile = $derived(status.platform === 'android' || status.platform === 'ios')

  function go(to: ScreenId) {
    error = ''
    transition(() => (screen = to))
    focusHeading()
    if (to === 'protect' && !key && status.phase === 'open' && status.recoveryPending) loadKey()
    if (to === 'done') loadFirst()
  }
  const next = () => go(nextScreen(screen))
  const back = () => go(prevScreen(screen))

  async function loadKey() {
    try {
      key = await call('recovery_key')
    } catch (e) {
      toast.error(e)
    }
  }
  // A wizard resumed at "protect" shows the key again (until it is confirmed saved).
  // svelte-ignore state_referenced_locally
  if (screen === 'protect' && status.phase === 'open') loadKey()

  async function create() {
    busy = true
    try {
      key = await call('create_store')
      await call('put_setting', { key: 'terms.accepted', value: true })
      await onstatus()
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }

  async function copyKey() {
    try {
      await navigator.clipboard.writeText(key)
      toast.show('Copied. Paste it somewhere safe, then clear your clipboard.')
    } catch {
      toast.show('Could not reach the clipboard; write the key down instead.', 'danger')
    }
  }

  async function protect() {
    if (!keySaved) {
      error = 'Tick the box once your recovery key is written down or saved.'
      return
    }
    error = checkPin(pin, pin2) ?? ''
    if (error) return
    busy = true
    try {
      if (pin) await call('set_pin', { pin })
      if (bio) await call('set_biometric', { on: true })
      await call('recovery_saved')
      pin = pin2 = ''
      await onstatus()
      next()
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }

  async function setUpdates(mode: UpdateMode) {
    busy = true
    try {
      await call('set_update_mode', { mode })
      next()
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }

  async function loadFirst() {
    try {
      first = (await call('steps_list')).steps[0] ?? null
    } catch {
      first = null
    }
  }

  async function finish() {
    busy = true
    try {
      await call('put_setting', { key: 'setup.done', value: true })
      await onfinish()
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }
</script>

<div class="wizard">
  <nav aria-label="Set-up progress">
    <ol class="progress">
      {#each SCREENS as s, i (s.id)}
        <li class:done={i < screenIndex(screen)} class:on={s.id === screen} aria-current={s.id === screen ? 'step' : undefined}>
          <span class="dot" aria-hidden="true">{#if i < screenIndex(screen)}<Check size={12} />{:else}{i + 1}{/if}</span>
          <span class="name">{s.title}</span>
        </li>
      {/each}
    </ol>
  </nav>

  <main class="page" id="main">
    <header class="page-head">
      <div>
        <p class="muted small">Step {screenIndex(screen) + 1} of {SCREENS.length}</p>
        <h1 tabindex="-1">{meta.title}</h1>
      </div>
    </header>
    <p class="why"><strong>Why we ask:</strong> <span>{meta.why}</span></p>

    {#if screen === 'welcome'}
      <Card>
        <p class="lead">Samriddhi shows what you own, earn and could earn, and suggests safe next steps, one at a time. Your data stays on your devices, encrypted.</p>
        <ol class="safety">
          {#each SAFETY as s, i (s.title)}
            <li><span class="n" aria-hidden="true">{i + 1}</span><div><strong>{s.title}</strong><p>{s.text}</p></div></li>
          {/each}
        </ol>
        <p class="muted small terms">Samriddhi is free, open-source software under the MIT licence, given as it is, without any warranty. You decide what to do with what it shows.</p>
        <label class="check"><input type="checkbox" bind:checked={accepted} /> I have read these six points and accept them.</label>
        <div class="row-actions">
          <Button disabled={!accepted} onclick={next}>Accept and continue</Button>
        </div>
      </Card>
    {:else if screen === 'protect'}
      <Card>
        {#if !key && status.phase === 'needsSetup'}
          <div class="stack">
            <p>We will now create your encrypted data store on this device and show its recovery key, once.</p>
            <div class="row-actions">
              <Button disabled={busy} onclick={create}>Create my encrypted store</Button>
              <Button variant="ghost" onclick={back}>Back</Button>
            </div>
          </div>
        {:else}
          <div class="stack">
            <div class="field">
              <span class="label" id="key-label">Your recovery key</span>
              <code class="key" aria-labelledby="key-label">{key ? groupKey(key) : '…'}</code>
              <span class="help">Write it on paper or keep it in a password manager. It opens your data on a new device, and we cannot show it again once you confirm.</span>
            </div>
            <div class="row-actions"><Button variant="quiet" onclick={copyKey} disabled={!key}>Copy the key</Button></div>
            <label class="check"><input type="checkbox" bind:checked={keySaved} /> I have written down or saved my recovery key.</label>

            <fieldset class="pin">
              <legend>App lock PIN (optional)</legend>
              <p class="help muted small">A PIN stops someone using the app if they pick up your unlocked device. Leave both empty to skip.</p>
              <div class="pair">
                <div class="field"><label for="pin1">PIN, 4 to 12 digits</label><input id="pin1" class="input num" type="password" inputmode="numeric" autocomplete="new-password" maxlength="12" bind:value={pin} /></div>
                <div class="field"><label for="pin2">Same PIN again</label><input id="pin2" class="input num" type="password" inputmode="numeric" autocomplete="new-password" maxlength="12" bind:value={pin2} /></div>
              </div>
              {#if mobile}
                <label class="check"><input type="checkbox" bind:checked={bio} /> Also unlock with fingerprint or face</label>
              {/if}
            </fieldset>

            {#if error}<p class="error" role="alert">{error}</p>{/if}
            <div class="row-actions">
              <Button disabled={busy || !key} onclick={protect}>Continue</Button>
            </div>
          </div>
        {/if}
      </Card>
    {:else if screen === 'countries'}
      <Card><CountriesEditor source="wizard" saveLabel="Save and continue" onsaved={next} /></Card>
    {:else if screen === 'goals'}
      <Card><GoalsEditor source="wizard" saveLabel="Save and continue" onsaved={next} /></Card>
    {:else if screen === 'money'}
      <Card><MoneyEditor source="wizard" saveLabel="Save and continue" onsaved={next} /></Card>
    {:else if screen === 'risk'}
      <Card>
        {#if risk}
          <div class="stack">
            <h2 class="sub">Your result</h2>
            <RiskResult result={risk} />
            <div class="row-actions"><Button onclick={next}>Continue</Button><Button variant="ghost" onclick={() => (risk = null)}>Change my answers</Button></div>
          </div>
        {:else}
          <RiskEditor saveLabel="See my result" onsaved={(r) => (r ? (risk = r) : next())} />
        {/if}
      </Card>
    {:else if screen === 'updates'}
      <Card>
        <div class="stack">
          <p>When Samriddhi starts, it can check the project's GitHub page for a new version and ask you before installing it. Nothing is installed until you press a button, and "Later" always works.</p>
          <p>Your data is backed up before every update, and you can go back to the version you had in Settings, Updates.</p>
          <p class="muted small">Checking tells GitHub your internet address and nothing else. Nothing about you or your money is sent. You can change this any time in Settings, Updates.</p>
          <div class="row-actions">
            <Button disabled={busy} onclick={() => setUpdates('ask')}>Ask me when a new version is out</Button>
            <Button variant="ghost" disabled={busy} onclick={() => setUpdates('off')}>Do not check</Button>
          </div>
        </div>
      </Card>
    {:else}
      <Card>
        <div class="stack">
          {#if first === undefined}
            <p class="muted">Working out your first step…</p>
          {:else if first}
            <p class="muted small">From your answers, this would help most right now:</p>
            <StepCard step={first} actions={false} />
          {:else}
            <p>Nothing urgent stands out from your answers. Your home screen will suggest steps as you add more.</p>
          {/if}
          <div class="row-actions"><Button disabled={busy} onclick={finish}>Go to my home</Button></div>
        </div>
      </Card>
    {/if}

    {#if screen !== 'welcome' && screen !== 'protect'}
      <div class="row-actions nav">
        <Button variant="ghost" onclick={back}>Back</Button>
        {#if screen !== 'done' && screen !== 'updates'}<button class="link-button" onclick={next}>Skip for now</button>{/if}
      </div>
    {/if}

    <Hint {...HINTS[screen]} />
  </main>
</div>

<style>
  .wizard { display: grid; grid-template-columns: 250px 1fr; min-height: 100vh; }
  .progress { list-style: none; padding: 32px 18px; display: flex; flex-direction: column; gap: 4px; position: sticky; top: 0; }
  .progress li { display: flex; gap: 10px; align-items: center; padding: 8px 10px; color: var(--c-muted); border-radius: var(--nav-on-radius); font-size: var(--text-small); }
  .progress li.on { background: var(--nav-on-bg); color: var(--nav-on-ink); font-weight: 600; }
  .progress li.done { color: var(--c-ink); }
  .dot { width: 22px; height: 22px; border-radius: 50%; display: grid; place-items: center; font-size: 11px; font-weight: 700; border: 1.5px solid currentColor; flex: 0 0 auto; }
  .done .dot { background: var(--c-ok); color: var(--c-panel); border-color: var(--c-ok); }
  .on .dot { background: var(--c-accent); color: var(--c-on-accent); border-color: var(--c-accent); }
  main { padding: 36px 40px 60px; max-width: 980px; min-width: 0; }
  .lead { font-size: var(--text-lead); margin-bottom: var(--space-lg); max-width: 68ch; }
  .safety { list-style: none; display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: var(--space-md); margin-bottom: var(--space-lg); }
  .safety li { display: flex; gap: var(--space-md); padding: var(--space-md); background: var(--row-bg); border-radius: var(--row-radius); border-bottom: var(--row-divider); }
  .safety .n { font-family: var(--font-number); font-weight: 700; color: var(--c-accent); font-size: var(--text-title); line-height: 1; min-width: 18px; }
  .safety strong { display: block; margin-bottom: 2px; }
  .safety p { color: var(--c-muted); font-size: var(--text-small); line-height: 1.5; }
  .terms { margin-bottom: var(--space-sm); }
  .key {
    display: block; font-family: var(--font-number); font-size: var(--text-lead); letter-spacing: .04em; word-spacing: .3em; line-height: 1.7;
    padding: var(--space-md) var(--space-lg); border-radius: var(--r-sm); background: var(--c-panel-2); border: 1px dashed var(--c-accent); user-select: all; overflow-wrap: anywhere;
  }
  .pin { border-top: 1px solid var(--c-line); padding-top: var(--space-md); }
  .pair { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: var(--space-md); max-width: 520px; margin-top: var(--space-sm); }
  .sub { font-size: var(--text-title); }
  .nav { margin-top: var(--space-lg); gap: var(--space-lg); }
  @media (max-width: 860px) {
    .wizard { grid-template-columns: 1fr; }
    .progress { flex-direction: row; overflow-x: auto; padding: 12px 16px; position: static; }
    .progress .name { display: none; }
    .progress li.on .name { display: inline; }
    main { padding: 16px 16px 48px; }
  }
</style>

<script lang="ts">
  // Picks the screen from the shell's status (route.ts), keeps the look in
  // step with the encrypted settings, and blends between screens.
  import { Shell, ui, type ModePref, type MotionPref } from '@samriddhi/ui'
  import House from '@lucide/svelte/icons/house'
  import SettingsIcon from '@lucide/svelte/icons/settings'
  import Lock from '@lucide/svelte/icons/lock'
  import { call, inApp, type Status } from './lib/api.ts'
  import { pageFromHash, viewFor, viewKey, type View } from './lib/route.ts'
  import { focusHeading, transition } from './lib/transition.ts'
  import { toast } from './lib/toast.svelte.ts'
  import Toasts from './components/Toasts.svelte'
  import UpdateBanner from './components/UpdateBanner.svelte'
  import { updates } from './lib/updates.svelte.ts'
  import Wizard from './screens/Wizard.svelte'
  import Unlock from './screens/Unlock.svelte'
  import Home from './screens/Home.svelte'
  import Settings from './screens/Settings.svelte'

  let status = $state<Status | null>(null)
  let setupDone = $state<boolean | null>(null)
  let hash = $state(location.hash)
  let failed = $state('')
  let view = $state<View>({ kind: 'loading' })
  let prefsLoaded = false

  const nav = [
    { id: 'home', label: 'Home', icon: House },
    { id: 'settings', label: 'Settings', icon: SettingsIcon },
  ]

  function show(next: View) {
    if (viewKey(next) === viewKey(view)) {
      view = next
      return
    }
    transition(() => (view = next))
    focusHeading()
  }

  async function refresh() {
    try {
      const s = await call('status')
      status = s
      if (s.notice) toast.show(s.notice)
      setupDone = s.phase === 'open' ? (await call('get_setting', { key: 'setup.done' })) === true : null
      if (s.phase === 'open') await loadPrefs()
      failed = ''
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e)
    }
    show(failed ? { kind: 'problem', text: failed } : viewFor(status, setupDone, hash))
  }

  // The look lives in the encrypted settings too, so a restore or a second
  // device brings it along. The browser copy only styles the lock screen.
  async function loadPrefs() {
    if (prefsLoaded) return
    const get = (key: string) => call('get_setting', { key }).catch(() => null)
    const [theme, mode, motion] = await Promise.all([get('ui.theme'), get('ui.mode'), get('ui.motion')])
    const change: { theme?: string; mode?: ModePref; motion?: MotionPref } = {}
    if (typeof theme === 'string' && ui.themes.some(([id]) => id === theme) && theme !== ui.theme) change.theme = theme
    if (typeof mode === 'string' && mode !== ui.mode) change.mode = mode as ModePref
    if (typeof motion === 'string' && motion !== ui.motion) change.motion = motion as MotionPref
    if (Object.keys(change).length) ui.set(change)
    prefsLoaded = true
  }

  $effect(() => {
    const prefs = { 'ui.theme': ui.theme, 'ui.mode': ui.mode, 'ui.motion': ui.motion }
    if (!prefsLoaded || status?.phase !== 'open') return
    for (const [key, value] of Object.entries(prefs)) call('put_setting', { key, value }).catch(() => {})
  })

  window.addEventListener('hashchange', () => {
    hash = location.hash
    if (view.kind === 'app') show(viewFor(status, setupDone, hash))
  })

  function go(page: string) {
    location.hash = page
  }

  async function lockNow() {
    try {
      if (await call('lock')) await refresh()
    } catch (e) {
      toast.error(e)
    }
  }

  // Check for a new version once the app is open, unlocked and set up. It
  // only asks; nothing is installed without the user pressing a button.
  $effect(() => {
    if (view.kind === 'app' && status?.phase === 'open') updates.startup()
  })

  refresh()
  const section = $derived(hash.split('/')[1] ?? 'look')
</script>

<a class="skip" href="#main" onclick={(e) => { e.preventDefault(); document.getElementById('main')?.focus() }}>Skip to content</a>

{#if view.kind === 'loading'}
  <main class="page center" id="main" aria-busy="true"><p class="muted">Opening Samriddhi…</p></main>
{:else if view.kind === 'problem'}
  <main class="page center" id="main"><h1 tabindex="-1">Samriddhi could not start</h1><p class="error">{view.text}</p></main>
{:else if view.kind === 'wizard' && status}
  <Wizard {status} resume={view.resume} onstatus={refresh} onfinish={async () => { location.hash = 'home'; hash = '#home'; await refresh() }} />
{:else if (view.kind === 'unlock' || view.kind === 'recover') && status}
  {#key view.kind}<Unlock {status} recover={view.kind === 'recover'} onunlocked={refresh} />{/key}
{:else if view.kind === 'app' && status}
  <Shell brand="Samriddhi" items={nav} current={view.page} onnavigate={go} tabs={2}>
    {#snippet footer()}
      <p>{inApp ? 'Local mode: your data stays on this device.' : 'Browser preview: made-up data, kept in memory only.'}</p>
      {#if status?.hasPin || status?.biometric}
        <button class="lock" onclick={lockNow}><Lock size={14} /> Lock now</button>
      {/if}
    {/snippet}
    <div class="page" id="main" tabindex="-1">
      <UpdateBanner />
      {#key view.page}
        {#if view.page === 'home'}
          <Home onanswer={() => { location.hash = 'settings/answers' }} />
        {:else}
          <Settings {status} section={section} onstatus={refresh} />
        {/if}
      {/key}
    </div>
  </Shell>
{/if}

<Toasts />

<style>
  .center { min-height: 100vh; display: grid; place-content: center; text-align: center; padding: 1rem; gap: 8px; }
  .page { outline: none; }
  .lock { margin-top: 8px; display: inline-flex; gap: 6px; align-items: center; border: 1px solid var(--c-line); background: none; color: var(--c-ink); border-radius: var(--r-control); padding: 6px 10px; cursor: pointer; font-size: var(--text-caption); font-weight: 600; }
</style>

<script lang="ts">
  // Look, lock, backups, updates, export and import, and your answers. Every change
  // that replaces data shows what it would change first and backs up before.
  import { Button, Card, Explain, Segmented, ThemePicker } from '@samriddhi/ui'
  import { call, chooseFolder, chooseSave, type Backups, type ImportPreview, type Status, type Summary } from '../lib/api.ts'
  import { HINTS } from '../lib/hints.ts'
  import { SAFETY, checkPin, checkRecoveryText } from '../lib/wizard.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import Hint from '../components/Hint.svelte'
  import CountriesEditor from '../components/CountriesEditor.svelte'
  import GoalsEditor from '../components/GoalsEditor.svelte'
  import MoneyEditor from '../components/MoneyEditor.svelte'
  import RiskEditor from '../components/RiskEditor.svelte'
  import UpdatesPanel from '../components/UpdatesPanel.svelte'

  type Section = 'look' | 'lock' | 'backups' | 'updates' | 'export' | 'answers' | 'safety'
  let { status, section: initial = 'look', onstatus }: { status: Status; section?: string; onstatus: () => Promise<void> } = $props()

  const SECTIONS: { value: Section; label: string }[] = [
    { value: 'look', label: 'Look' },
    { value: 'lock', label: 'Lock' },
    { value: 'backups', label: 'Backups' },
    { value: 'updates', label: 'Updates' },
    { value: 'export', label: 'Export' },
    { value: 'answers', label: 'Your answers' },
    { value: 'safety', label: 'Safety' },
  ]
  // svelte-ignore state_referenced_locally
  let section = $state<Section>(SECTIONS.some((s) => s.value === initial) ? (initial as Section) : 'look')

  const TABLES: Record<string, string> = {
    sam_profile: 'Facts about your money',
    sam_countries: 'Countries',
    sam_goals: 'Goals',
    sam_risk_answers: 'Risk answers',
    sam_steps_dismissed: 'Hidden steps',
    sam_settings: 'Settings',
    sam_audit: 'Activity log',
    sam_meta: 'Data version',
  }
  const tableName = (t: string) => TABLES[t] ?? t
  const kb = (n: number) => `${Math.max(1, Math.round(n / 1024))} KB`
  const KIND = { daily: 'Daily', manual: 'Made by you', preUpdate: 'Before an update', beforeRestore: 'Before a restore', beforeImport: 'Before an import' }

  async function act<T>(f: () => Promise<T>, done?: (v: T) => string) {
    try {
      const v = await f()
      if (done) toast.show(done(v))
      return v
    } catch (e) {
      toast.error(e)
      return undefined
    }
  }

  // Lock
  let pin = $state('')
  let pin2 = $state('')
  let pinError = $state('')
  const mobile = $derived(status.platform === 'android' || status.platform === 'ios')
  async function savePin(e: SubmitEvent) {
    e.preventDefault()
    pinError = checkPin(pin, pin2) ?? (pin ? '' : 'Type a PIN of 4 to 12 digits.')
    if (pinError) return
    await act(() => call('set_pin', { pin }), () => (status.hasPin ? 'PIN changed.' : 'PIN set. The app now locks when it starts.'))
    pin = pin2 = ''
    await onstatus()
  }
  async function removePin() {
    await act(() => call('clear_pin'), () => 'PIN removed.')
    await onstatus()
  }
  async function setBio(on: boolean) {
    await act(() => call('set_biometric', { on }), () => (on ? 'Fingerprint or face unlock is on.' : 'Fingerprint or face unlock is off.'))
    await onstatus()
  }
  async function lockNow() {
    const locked = await act(() => call('lock'))
    if (locked) await onstatus()
    else if (locked === false) toast.show('Set a PIN first; without one there is nothing to unlock with.', 'danger')
  }

  // Backups
  let backups = $state<Backups | null>(null)
  const loadBackups = async () => (backups = (await act(() => call('backups'))) ?? backups)
  $effect(() => {
    if (section === 'backups') loadBackups()
  })
  async function backupNow() {
    await act(() => call('backup_now'), (n) => `Backed up as ${n}.`)
    await loadBackups()
  }
  async function saveAs() {
    const target = await chooseSave('Save a backup as', 'samriddhi-backup.db', 'db')
    if (target) await act(() => call('save_backup_to', { target }), () => 'Backup saved. It opens only with this device or your recovery key.')
  }
  async function chooseExtra() {
    const folder = await chooseFolder('Also keep daily backups in')
    if (folder) await act(() => call('set_backup_folder', { folder }), () => 'Daily backups will also go to that folder.')
    await loadBackups()
  }
  async function stopExtra() {
    await act(() => call('set_backup_folder', { folder: null }), () => 'No longer copying backups to the extra folder.')
    await loadBackups()
  }

  // Restore: pick, preview, confirm.
  let restore = $state<{ name: string | null; label: string; summary: Summary | null; recovery: string; error: string } | null>(null)
  async function previewRestore(name: string | null, label: string) {
    restore = { name, label, summary: null, recovery: '', error: '' }
    await inspect()
  }
  async function pickRestore() {
    const label = await act(() => call('pick_backup'))
    if (label) await previewRestore(null, label)
  }
  async function inspect() {
    if (!restore) return
    if (restore.recovery.trim()) {
      const bad = checkRecoveryText(restore.recovery)
      if (bad) {
        restore.error = bad
        return
      }
    }
    try {
      restore.summary = await call('inspect_backup', { name: restore.name, recovery: restore.recovery.trim() || null })
      restore.error = ''
    } catch (e) {
      restore.summary = null
      restore.error = e instanceof Error ? e.message : String(e)
    }
  }
  async function doRestore() {
    if (!restore) return
    const r = restore
    const saved = await act(
      () => call('restore_backup', { name: r.name, recovery: r.recovery.trim() || null }),
      (n) => `Restored. Your data from before was kept as ${n}.`,
    )
    if (saved) {
      restore = null
      await loadBackups()
      await onstatus()
    }
  }

  // Export and import
  let preview = $state<ImportPreview | null>(null)
  const stamp = () => new Date().toISOString().slice(0, 10)
  async function exportJson() {
    const target = await chooseSave('Export everything as JSON', `samriddhi-export-${stamp()}.json`, 'json')
    if (target) await act(() => call('export_json', { target }), () => 'Exported. The file is readable, so keep it somewhere private.')
  }
  async function exportCsv() {
    const folder = await chooseFolder('Export CSV files into')
    if (folder) await act(() => call('export_csv', { folder }), (dir) => `Exported one CSV file per table into ${dir}.`)
  }
  async function pickImport() {
    preview = (await act(() => call('import_pick'))) ?? null
  }
  async function applyImport() {
    const saved = await act(() => call('import_apply'), (n) => `Imported. Your data from before was kept as backup ${n}.`)
    if (saved) {
      preview = null
      await onstatus()
    }
  }
  async function cancelImport() {
    await act(() => call('import_cancel'))
    preview = null
  }

  // Your answers
  type Part = 'countries' | 'goals' | 'money' | 'risk'
  let part = $state<Part>('money')
  const saved = () => toast.show('Saved. Your home screen and steps now use it.')
</script>

<header class="page-head">
  <div>
    <h1 tabindex="-1">Settings</h1>
    <p>How Samriddhi looks and locks, your backups and exports, and the answers you gave.</p>
  </div>
</header>

<div class="tabs"><Segmented label="Settings section" options={SECTIONS} value={section} onchange={(v) => (section = v)} /></div>

{#if section === 'look'}
  <Card title="Theme" explain="Three looks, each with light and dark. Calm motion turns off movement and blending between screens; Match device follows your system's reduced-motion setting.">
    <ThemePicker />
  </Card>
{:else if section === 'lock'}
  <div class="grid">
    <Card title={status.hasPin ? 'Change your PIN' : 'Set a PIN'} explain="The PIN guards the screen: Samriddhi asks for it when it starts or when you lock it. Your data is encrypted either way, with a key this device keeps.">
      <form class="stack" onsubmit={savePin}>
        <div class="field"><label for="s-pin1">New PIN, 4 to 12 digits</label><input id="s-pin1" class="input num" type="password" inputmode="numeric" autocomplete="new-password" maxlength="12" bind:value={pin} /></div>
        <div class="field"><label for="s-pin2">Same PIN again</label><input id="s-pin2" class="input num" type="password" inputmode="numeric" autocomplete="new-password" maxlength="12" bind:value={pin2} /></div>
        {#if pinError}<p class="error" role="alert">{pinError}</p>{/if}
        <div class="row-actions">
          <Button type="submit">{status.hasPin ? 'Change PIN' : 'Set PIN'}</Button>
          {#if status.hasPin}<Button variant="ghost" onclick={removePin}>Remove PIN</Button>{/if}
        </div>
      </form>
    </Card>
    <Card title="Unlock and lock" explain="Fingerprint or face unlock works on phones. On a computer, use a PIN. Your recovery key always opens the app too.">
      <div class="stack">
        {#if mobile}
          <label class="check"><input type="checkbox" checked={status.biometric} onchange={(e) => setBio(e.currentTarget.checked)} /> Unlock with fingerprint or face</label>
        {:else}
          <p class="muted small">Fingerprint and face unlock are for phones; this computer uses a PIN.</p>
        {/if}
        <div><Button variant="quiet" disabled={!status.hasPin && !status.biometric} onclick={lockNow}>Lock now</Button></div>
      </div>
    </Card>
  </div>
{:else if section === 'backups'}
  <div class="grid">
    <Card title="Backups" explain="Every day Samriddhi keeps an encrypted copy of your data on this device (the newest 14), and one before each update (the newest 3). They open only with this device's key or your recovery key.">
      <div class="stack">
        <div class="row-actions">
          <Button onclick={backupNow}>Back up now</Button>
          <Button variant="ghost" onclick={saveAs}>Save a backup as…</Button>
        </div>
        {#if backups}
          <p class="muted small">Kept in <span class="path">{backups.folder}</span></p>
          <ul class="files">
            {#each backups.files as f (f.name)}
              <li>
                <span><span class="num">{f.name}</span><small class="muted">{KIND[f.kind]} · {kb(f.bytes)}</small></span>
                <Button variant="ghost" onclick={() => previewRestore(f.name, f.name)}>Restore…</Button>
              </li>
            {:else}
              <li class="muted">No backups yet.</li>
            {/each}
          </ul>
        {/if}
      </div>
    </Card>
    <div class="stack">
      <Card title="A second copy" explain="Keep the daily backup in another folder too, such as a synced or external drive, so losing this device does not lose your backups.">
        {#if backups?.userFolder}
          <p class="small">Also copying to <span class="path">{backups.userFolder}</span> ({backups.userFiles.length} there).</p>
          <div class="row-actions"><Button variant="ghost" onclick={chooseExtra}>Change folder</Button><Button variant="ghost" onclick={stopExtra}>Stop</Button></div>
        {:else}
          <p class="small muted">Not set.</p>
          <div class="row-actions"><Button variant="quiet" onclick={chooseExtra}>Choose a folder</Button></div>
        {/if}
      </Card>
      <Card title="Restore" explain="Put a backup back in place of your current data. You see what it holds first, and your current data is backed up before anything changes.">
        <div class="stack">
          {#if !restore}
            <p class="small muted">Pick a backup from the list, or a file from elsewhere (another device needs that device's recovery key).</p>
            <div><Button variant="quiet" onclick={pickRestore}>Choose a file…</Button></div>
          {:else}
            <p class="small">From <span class="num">{restore.label}</span></p>
            {#if restore.summary}
              <table class="counts">
                <caption class="sr-only">What the backup holds</caption>
                <thead><tr><th scope="col">What</th><th scope="col" class="n">Rows</th></tr></thead>
                <tbody>
                  {#each Object.entries(restore.summary.counts) as [t, n] (t)}<tr><td>{tableName(t)}</td><td class="n">{n}</td></tr>{/each}
                </tbody>
              </table>
              <p class="note-box warn small">This replaces everything you have now. Your current data is backed up first.</p>
              <div class="row-actions"><Button onclick={doRestore}>Restore this backup</Button><Button variant="ghost" onclick={() => (restore = null)}>Cancel</Button></div>
            {:else}
              {#if restore.error}<p class="error" role="alert">{restore.error}</p>{/if}
              <div class="field">
                <label for="restore-key">Recovery key of the device it came from (if asked)</label>
                <textarea id="restore-key" class="input num" rows="2" spellcheck="false" bind:value={restore.recovery}></textarea>
              </div>
              <div class="row-actions"><Button variant="quiet" onclick={inspect}>Check it</Button><Button variant="ghost" onclick={() => (restore = null)}>Cancel</Button></div>
            {/if}
          {/if}
        </div>
      </Card>
    </div>
  </div>
{:else if section === 'updates'}
  <UpdatesPanel onbackups={() => (section = 'backups')} />
{:else if section === 'export'}
  <div class="grid">
    <Card title="Export everything" explain="A full, readable copy of everything Samriddhi holds about you. Yours to keep, move or read; no paywall, no lock-in.">
      <div class="stack">
        <p class="small">JSON is one file with everything and can be imported back. CSV is one file per table, for spreadsheets.</p>
        <div class="row-actions"><Button onclick={exportJson}>Export as JSON</Button><Button variant="ghost" onclick={exportCsv}>Export as CSV</Button></div>
        <p class="note-box small">Exports are not encrypted. Keep them somewhere private.</p>
      </div>
    </Card>
    <Card title="Import" explain="Read a Samriddhi JSON export back in. You see how many rows of each kind it would bring before anything changes, and your current data is backed up first.">
      <div class="stack">
        {#if !preview}
          <div><Button variant="quiet" onclick={pickImport}>Choose an export file…</Button></div>
        {:else}
          <p class="small">Export from <span class="num">{preview.exportedAt.slice(0, 16).replace('T', ' ')}</span> (data version {preview.schema}).</p>
          <table class="counts">
            <caption class="sr-only">What importing would change</caption>
            <thead><tr><th scope="col">What</th><th scope="col" class="n">Now</th><th scope="col" class="n">After import</th></tr></thead>
            <tbody>
              {#each preview.tables as t (t.table)}<tr><td>{tableName(t.table)}</td><td class="n">{t.current}</td><td class="n">{t.incoming}</td></tr>{/each}
            </tbody>
          </table>
          {#if preview.kept.length}<p class="muted small">Kept as they are on this device: {preview.kept.map(tableName).join(', ')}.</p>{/if}
          <div class="row-actions"><Button onclick={applyImport}>Import and replace</Button><Button variant="ghost" onclick={cancelImport}>Cancel</Button></div>
        {/if}
      </div>
    </Card>
  </div>
{:else if section === 'answers'}
  <Card title="Your answers" explain="Change anything you told Samriddhi. Changes are used straight away for your net worth and next steps.">
    <div class="tabs"><Segmented label="Which answers" options={[{ value: 'money', label: 'Money' }, { value: 'countries', label: 'Countries' }, { value: 'goals', label: 'Goals' }, { value: 'risk', label: 'Risk' }]} value={part} onchange={(v) => (part = v as Part)} /></div>
    {#key part}
      {#if part === 'money'}<MoneyEditor source="asked" saveLabel="Save" onsaved={saved} />
      {:else if part === 'countries'}<CountriesEditor source="asked" saveLabel="Save" onsaved={saved} />
      {:else if part === 'goals'}<GoalsEditor source="asked" saveLabel="Save" onsaved={saved} />
      {:else}<RiskEditor saveLabel="Save my answers" onsaved={saved} />{/if}
    {/key}
  </Card>
{:else}
  <Card title="What Samriddhi will and will not do" explain="The six points you accepted at set-up. They are built into the app, not just written here.">
    <ol class="safety">
      {#each SAFETY as s (s.title)}<li><strong>{s.title}.</strong> <span class="muted">{s.text}</span></li>{/each}
    </ol>
    <p class="muted small about">Samriddhi 0.1.0 · local mode: your data stays on this device · MIT licence <Explain title="Local mode" text="In this version everything runs on this device: no bank links, no AI, no tracking. The only time it goes online is to check GitHub for a new version, which you can turn off in Settings, Updates." /></p>
  </Card>
{/if}

<Hint {...HINTS.settings} />

<style>
  .tabs { margin-bottom: var(--space-lg); overflow-x: auto; }
  .files { list-style: none; display: flex; flex-direction: column; }
  .files li { display: flex; justify-content: space-between; align-items: center; gap: var(--space-md); padding: 8px 0; border-bottom: 1px solid var(--c-line); }
  .files small { display: block; }
  .path { font-family: var(--font-number); overflow-wrap: anywhere; color: var(--c-ink); }
  .safety { padding-left: 1.3em; display: flex; flex-direction: column; gap: var(--space-sm); max-width: 80ch; line-height: 1.5; }
  .about { margin-top: var(--space-lg); display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
</style>

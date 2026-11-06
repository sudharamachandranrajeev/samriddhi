<script lang="ts">
  // Settings > Updates: the check-at-start setting, going back a version, and
  // reporting a problem or suggesting a feature as a prefilled GitHub issue.
  // Ported from Arogya Swasthya.
  import { Button, Card, ui } from '@samriddhi/ui'
  import { call, inApp, type Page, type RollbackPlan, type UpdateMode } from '../lib/api.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import { kindName, sizeText, updates } from '../lib/updates.svelte.ts'

  let { onbackups }: { onbackups?: () => void } = $props()

  let plan = $state<RollbackPlan | null>(null)
  let confirming = $state(false)
  let checking = $state(false)
  const s = $derived(updates.status)

  $effect(() => {
    updates
      .load()
      .then(() => call('rollback_plan'))
      .then((p) => (plan = p))
      .catch((e) => toast.error(e))
  })

  async function setMode(mode: UpdateMode) {
    try {
      await call('set_update_mode', { mode })
      await updates.load()
      toast.show(mode === 'ask' ? 'Samriddhi will check for new versions when it starts.' : 'Samriddhi will not check for new versions.')
    } catch (e) {
      toast.error(e)
    }
  }

  async function checkNow() {
    checking = true
    updates.dismissed = false
    await updates.check()
    checking = false
  }

  async function report(page: Page) {
    try {
      await call('open_page', { page, fields: { theme: ui.theme, errors: toast.recentErrors().join('\n') } })
    } catch (e) {
      toast.error(e)
    }
  }
</script>

<div class="grid">
  <Card title="Updates" explain="New versions come from the project's GitHub releases. Every download is checked against the project's signature before it is installed, and your data is backed up first.">
    {#if s}
      <div class="stack">
        <p>
          This is Samriddhi {s.current}, {kindName[s.kind]}.
          {#if s.canInstall}It can update itself.{:else}It cannot update itself, but it tells you when a new version is out.{/if}
        </p>
        <div class="field">
          <label for="u-mode">When Samriddhi starts</label>
          <select id="u-mode" class="input" value={s.mode} onchange={(e) => setMode(e.currentTarget.value as UpdateMode)}>
            <option value="ask">Check for a new version and ask me before installing</option>
            <option value="off">Do not check</option>
          </select>
          <span class="help">Checking asks GitHub for the list of the newest version and sends nothing about you or your money. Offline, the check is skipped quietly.</span>
        </div>
        <div class="row-actions">
          <Button variant="quiet" disabled={checking || !inApp} onclick={checkNow}>{checking ? 'Checking…' : 'Check now'}</Button>
        </div>
        {#if updates.found}
          <p class="small">Version {updates.found.version} is out{updates.found.size ? ` (${sizeText(updates.found.size)})` : ''}: see the message at the top of the page.</p>
        {/if}
      </div>
    {/if}
  </Card>

  <Card title="Go back a version" explain="If a new version does not suit you, go back to the one you had before. Nothing is ever forced on you.">
    {#if s}
      <div class="stack">
        {#if s.kind === 'android'}
          <p class="small">Android cannot install an older version over a newer one. To go back on a phone, restore the backup made before the update: it is listed under Backups as "Before an update".</p>
          {#if onbackups}<div><Button variant="quiet" onclick={onbackups}>Open Backups</Button></div>{/if}
        {:else if !plan}
          <p class="small muted">There is no earlier version to go back to yet. This appears after Samriddhi updates itself.</p>
        {:else if !s.canInstall}
          <p class="small">This copy cannot install versions itself. Download {plan.version} from the release page.</p>
        {:else}
          <p class="small">Before the last update you had version {plan.version}. Going back downloads it, checks it the same way as an update, backs up your data, and restarts.</p>
          {#if plan.needsRestore}
            <p class="note-box warn small">Version {plan.version} cannot read your data as it is now. Going back puts the backup from before the update in its place at the next start, so changes made since the update are lost. Your current data is backed up first.</p>
          {/if}
          {#if confirming}
            <div class="row-actions">
              <Button disabled={updates.busy} onclick={() => updates.rollback(plan!.needsRestore)}>{updates.busy ? 'Working…' : `Go back to ${plan.version}`}</Button>
              <Button variant="ghost" disabled={updates.busy} onclick={() => (confirming = false)}>Cancel</Button>
            </div>
            {#if updates.progress !== null}<progress max="1" value={updates.progress} aria-label="Download"></progress>{/if}
          {:else}
            <div><Button variant="quiet" onclick={() => (confirming = true)}>Go back to version {plan.version}…</Button></div>
          {/if}
        {/if}
      </div>
    {/if}
  </Card>

  <Card title="Tell the makers" explain="Opens a new GitHub issue in your browser with the version, system, install type, theme and the last three error messages filled in. Amounts, names and numbers are taken out of the messages. Nothing is sent until you press Submit on GitHub.">
    <div class="stack">
      <p class="small">Something not working, or an idea that would help? You see everything on GitHub before you send it, and you can change or remove any of it.</p>
      <p class="small muted">Never included: amounts, account names, people, or anything you told Samriddhi about yourself.</p>
      <div class="row-actions">
        <Button variant="quiet" disabled={!inApp} onclick={() => report('bug')}>Report a problem</Button>
        <Button variant="quiet" disabled={!inApp} onclick={() => report('idea')}>Suggest a feature</Button>
      </div>
    </div>
  </Card>
</div>

<style>
  progress { width: 100%; accent-color: var(--c-accent); }
</style>

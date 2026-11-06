<script lang="ts">
  // One next best step: what, why, how risky, how much effort, where the
  // facts come from and when they were checked. "Later" hides it for 14
  // days; "Not for me" hides it and keeps your reason. Steps from the agents
  // also say who suggested them and how far to trust them, and "Accept" on
  // an action only notes the choice: nothing is done for you.
  import { Button, Explain, Pill } from '@samriddhi/ui'
  import { call, type PensionMatch, type Step } from '../lib/api.ts'
  import { acceptable, isAddress, originLabel } from '../lib/feed.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import Disclosure from './Disclosure.svelte'

  let {
    step,
    onchange,
    onanswer,
    actions = true,
  }: { step: Step; onchange?: () => void; onanswer?: (fact: string) => void; actions?: boolean } = $props()

  let reasonOpen = $state(false)
  let reason = $state('')
  let busy = $state(false)

  const RISK = ['', 'Nothing can be lost', 'Little can be lost', 'Some can be lost', 'Much could be lost', 'Most could be lost']
  const EFFORT = { minutes: 'A few minutes', hours: 'An hour or two', ongoing: 'A habit over months' }
  const host = $derived(step.source.replace(/^https?:\/\/(www\.)?/, '').replace(/\/$/, ''))

  async function run<T>(f: () => Promise<T>, done: string) {
    busy = true
    try {
      await f()
      toast.show(done)
      onchange?.()
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }

  const accept = () =>
    run(() => call('step_accept', { id: step.id }), 'Noted as your choice. Nothing was done for you: acting on it is up to you.')
  const later = () => run(() => call('step_later', { id: step.id }), 'Hidden for 14 days. Find it again under hidden steps.')
  const notForMe = () =>
    run(() => call('step_not_for_me', { id: step.id, reason }), 'Hidden. Thank you for the reason; it helps order future steps.')
  const pension = (v: PensionMatch) =>
    run(() => call('set_fact', { key: 'pension.employerMatch', value: v, source: 'asked' }), 'Saved.')
</script>

<article class="step" aria-labelledby={`step-${step.id}`}>
  {#if step.origin !== 'steps'}
    <p class="origin small"><Pill tone="done">{originLabel(step.origin)}</Pill> <span class="muted">Suggested by the {originLabel(step.origin).toLowerCase()} check</span></p>
  {/if}
  <h3 id={`step-${step.id}`}>{step.title}</h3>
  <p class="why">{step.why}</p>

  <div class="facts">
    <span class="fact">
      Risk
      <span class="meter" role="img" aria-label={`${step.risk} of 5: ${RISK[step.risk]}`}>{#each [1, 2, 3, 4, 5] as n (n)}<i class:on={n <= step.risk}></i>{/each}</span>
      <span class="muted">{RISK[step.risk]}</span>
      <Explain title="Risk of this step" text="From 1 (nothing you put in can be lost, such as paying off debt or saving cash) to 5 (you could lose much of it). Steps never go above the level your risk answers allow." />
    </span>
    <Pill tone="accent">{EFFORT[step.effort]}</Pill>
  </div>
  <p class="src small muted">
    <!-- Plain text, not a link: the app opens no web pages itself (no network in Phase 1). -->
    {#if isAddress(step.source)}
      Facts from <span class="url" title={step.source}>{host}</span>, checked {step.asOf}.
    {:else}
      Based on {step.source}, as of {step.asOf}.
    {/if}
  </p>
  {#if step.basis}
    <p class="basis small"><strong>How far to trust it:</strong> {step.basis} <Explain title="How far to trust it" text="Every suggestion from Samriddhi's checks says what it rests on: your own answers, exact sums on your holdings, or a model on made-up data. The weaker the basis, the more you should check before acting." /></p>
  {/if}
  {#if step.disclosure}<Disclosure text={step.disclosure} />{/if}

  {#if step.ask?.fact === 'pension.employerMatch' && actions}
    <fieldset class="ask">
      <legend>Your answer</legend>
      <div class="row-actions">
        <Button variant="quiet" disabled={busy} onclick={() => pension('yes')}>Yes, it does</Button>
        <Button variant="quiet" disabled={busy} onclick={() => pension('no')}>No</Button>
        <Button variant="quiet" disabled={busy} onclick={() => pension('notEmployed')}>I am not employed</Button>
        <Button variant="ghost" disabled={busy} onclick={() => pension('unsure')}>Not sure yet</Button>
      </div>
    </fieldset>
  {:else if step.ask && actions && onanswer}
    <div class="row-actions"><Button onclick={() => onanswer?.(step.ask!.fact)}>Answer it</Button></div>
  {/if}

  {#if actions}
    {#if acceptable(step)}
      <p class="small muted" id={`accept-${step.id}`}>Accept only notes on this device that you mean to do it. Samriddhi never trades, moves money or contacts anyone for you.</p>
    {/if}
    <div class="row-actions">
      {#if acceptable(step)}
        <Button variant="quiet" disabled={busy} onclick={accept}>Accept (note my choice)</Button>
      {/if}
      <Button variant="ghost" disabled={busy} onclick={later}>Later</Button>
      <Button variant="ghost" disabled={busy} onclick={() => (reasonOpen = !reasonOpen)}>Not for me</Button>
    </div>
    {#if reasonOpen}
      <div class="field reason">
        <label for={`reason-${step.id}`}>Why not? (optional, stays on this device)</label>
        <textarea id={`reason-${step.id}`} class="input" maxlength="500" bind:value={reason} placeholder="For example: I already do this"></textarea>
        <div class="row-actions">
          <Button disabled={busy} onclick={notForMe}>Hide this step</Button>
          <Button variant="ghost" onclick={() => (reasonOpen = false)}>Cancel</Button>
        </div>
      </div>
    {/if}
  {/if}
</article>

<style>
  .step { display: flex; flex-direction: column; gap: var(--space-sm); }
  h3 { font-size: var(--text-title); line-height: 1.3; }
  .why { color: var(--c-muted); line-height: 1.55; }
  .facts { display: flex; flex-wrap: wrap; gap: var(--space-md); align-items: center; font-size: var(--text-small); }
  .fact { display: inline-flex; align-items: center; gap: 8px; }
  .url { color: var(--c-ink); user-select: all; overflow-wrap: anywhere; }
  .ask legend { font-size: var(--text-small); }
  .reason { margin-top: var(--space-sm); }
  .origin { display: flex; align-items: center; gap: var(--space-sm); flex-wrap: wrap; }
  .basis { line-height: 1.5; display: block; }
  .basis strong { font-weight: 600; }
</style>

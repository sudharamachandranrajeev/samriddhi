<script lang="ts">
  // Optional AI narration, off by default. You choose where it goes (your own
  // model, an OpenAI-compatible service or Anthropic), the key goes to the
  // device keyring and is never shown again, and you read exactly what would
  // be sent before a separate Send. The reply is shown as plain text only.
  import { Button, Card, Explain, Segmented } from '@samriddhi/ui'
  import { call, type AiPreview, type AiProvider, type AiSettingsView } from '../lib/api.ts'
  import { toast } from '../lib/toast.svelte.ts'

  const PROVIDERS: { value: AiProvider; label: string }[] = [
    { value: 'local', label: 'My own model' },
    { value: 'cloud', label: 'Cloud service' },
    { value: 'anthropic', label: 'Anthropic' },
  ]

  let view = $state<AiSettingsView | null>(null)
  let enabled = $state(false)
  let provider = $state<AiProvider>('local')
  let endpoint = $state('')
  let model = $state('')
  let key = $state('')
  let preview = $state<AiPreview | null>(null)
  let reply = $state('')
  let busy = $state(false)

  async function load() {
    try {
      const v = await call('ai_settings_get')
      view = v
      enabled = v.settings.enabled
      provider = v.settings.provider
      // An empty address starts as the usual one for the provider, shown and editable.
      endpoint = v.settings.endpoint || (provider === 'local' ? v.localSuggestion : provider === 'anthropic' ? v.anthropicSuggestion : '')
      model = v.settings.model || (provider === 'anthropic' ? v.anthropicModel : '')
    } catch (e) {
      toast.error(e)
    }
  }
  load()

  function pickProvider(p: AiProvider) {
    provider = p
    preview = null
    reply = ''
    if (!view) return
    // Offer the usual address for a provider when the box is empty or still holds another's suggestion.
    const suggested = [view.localSuggestion, view.anthropicSuggestion, '']
    if (suggested.includes(endpoint.trim())) endpoint = p === 'local' ? view.localSuggestion : p === 'anthropic' ? view.anthropicSuggestion : ''
    if (p === 'anthropic' && !model.trim()) model = view.anthropicModel
  }

  async function run(f: () => Promise<unknown>, done?: string) {
    busy = true
    try {
      await f()
      if (done) toast.show(done)
      await load()
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }

  function save(e: SubmitEvent) {
    e.preventDefault()
    preview = null
    reply = ''
    run(() => call('ai_settings_set', { settings: { enabled, provider, endpoint: endpoint.trim(), model: model.trim() } }), enabled ? 'AI settings saved.' : 'AI is off.')
  }

  function saveKey(e: SubmitEvent) {
    e.preventDefault()
    if (!key.trim()) return
    const k = key.trim()
    key = ''
    run(() => call('ai_key_set', { key: k }), 'Key saved in this device’s keyring.')
  }

  async function makePreview() {
    reply = ''
    busy = true
    try {
      preview = await call('ai_preview')
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }

  async function send() {
    if (!preview) return
    busy = true
    try {
      reply = await call('ai_send', { text: preview.text })
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }

  const needsKey = $derived(provider !== 'local')
</script>

<div class="grid">
  <Card title="AI narration" explain="An optional AI model can put your next steps and figures into plain words. It is off unless you turn it on. Samriddhi works fully without it, and the model never decides or does anything.">
    {#if !view}
      <p class="muted">Loading…</p>
    {:else}
      <form class="stack" onsubmit={save}>
        <label class="check"><input type="checkbox" bind:checked={enabled} /> Use an AI model to explain my steps</label>
        <div class="field">
          <span class="label" id="ai-prov">Where it goes</span>
          <Segmented label="AI provider" options={PROVIDERS} value={provider} onchange={(v) => pickProvider(v as AiProvider)} />
          <p class="muted small">
            {#if provider === 'local'}A model running on this device or your own network, such as Ollama. Nothing leaves your network.
            {:else if provider === 'cloud'}A service with an OpenAI-compatible address. What you send goes to that company.
            {:else}Anthropic's API. What you send goes to Anthropic.{/if}
          </p>
        </div>
        <div class="field"><label for="ai-endpoint">Address</label><input id="ai-endpoint" class="input num" autocomplete="off" spellcheck="false" bind:value={endpoint} placeholder={provider === 'local' ? view.localSuggestion : provider === 'anthropic' ? view.anthropicSuggestion : 'https://'} /></div>
        <div class="field"><label for="ai-model">Model</label><input id="ai-model" class="input num" autocomplete="off" spellcheck="false" bind:value={model} placeholder={provider === 'anthropic' ? view.anthropicModel : 'Model name'} /></div>
        {#if view.problem}<p class="note-box warn small">{view.problem}</p>{/if}
        <div class="row-actions"><Button type="submit" disabled={busy}>Save AI settings</Button></div>
      </form>
    {/if}
  </Card>

  {#if view}
    <div class="stack">
      {#if needsKey}
        <Card title="Key" explain="The service's key is kept in this device's keyring (the Keystore on Android), never in Samriddhi's data or backups. Once saved it is never shown again.">
          {#if view.keySet}
            <p class="small"><strong>Key saved.</strong> It stays in the keyring and is not shown.</p>
            <div class="row-actions"><Button variant="ghost" disabled={busy} onclick={() => run(() => call('ai_key_clear'), 'Key removed from the keyring.')}>Remove key</Button></div>
          {:else}
            <form class="stack" onsubmit={saveKey}>
              <div class="field"><label for="ai-key">Key</label><input id="ai-key" class="input num" type="password" autocomplete="off" spellcheck="false" bind:value={key} /></div>
              {#if view.keyProblem}<p class="note-box warn small">{view.keyProblem}</p>{/if}
              <div class="row-actions"><Button type="submit" variant="quiet" disabled={busy || !key.trim()}>Save key</Button></div>
            </form>
          {/if}
        </Card>
      {/if}

      <Card title="See what would be sent" explain="Samriddhi builds a short text from your next steps: figures rounded, no names of accounts or holdings. You read all of it first; nothing goes anywhere until you press Send.">
        <div class="stack">
          <div><Button variant="quiet" disabled={busy || !view.settings.enabled} onclick={makePreview}>Show what would be sent</Button></div>
          {#if !view.settings.enabled}<p class="muted small">Turn AI on and save first.</p>{/if}
          {#if preview}
            <p class="small">To <span class="num">{preview.destination}</span> <Explain title="Destination" text="The address the text would go to, from your settings. Nothing is sent until you press Send." /></p>
            <h4 class="small">Instructions to the model</h4>
            <pre class="sent">{preview.system}</pre>
            <h4 class="small">Your text</h4>
            <pre class="sent">{preview.text}</pre>
            <div class="row-actions">
              <Button disabled={busy || !view.canSend} onclick={send}>Send this text</Button>
              <Button variant="ghost" onclick={() => (preview = null)}>Do not send</Button>
            </div>
          {/if}
          {#if reply}
            <section aria-label="What the model said" class="reply">
              <h4 class="small">What the model said</h4>
              <p class="reply-text">{reply}</p>
              <p class="muted small">Words from a model, not advice. Check anything that matters.</p>
            </section>
          {/if}
        </div>
      </Card>
    </div>
  {/if}
</div>

<style>
  .label { font-weight: 600; font-size: var(--text-small); }
  .sent {
    white-space: pre-wrap; overflow-wrap: anywhere; max-height: 320px; overflow: auto; margin: 0;
    padding: var(--space-sm) var(--space-md); border: 1px solid var(--c-line); border-radius: var(--r-sm);
    font-family: var(--font-number); font-size: var(--text-caption); line-height: 1.5;
  }
  .reply { border-top: 1px solid var(--c-line); padding-top: var(--space-md); }
  .reply-text { white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.55; }
</style>

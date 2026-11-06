<script lang="ts">
  // Make your own theme from any existing one. Changes show live in the preview; the contrast
  // check says in plain words when text would be hard to read.
  import { ui } from '../theme.svelte.ts'
  import { tokens } from '../generated/tokens.ts'
  import { colourNames, cssBlock, themeSelector, themeVars, type Mode, type ThemeDef } from '../compile.ts'
  import { check } from '../contrast.ts'
  import { applyPreset, colourHelp, pairWords, presetChoices, presetOf, styleGroups, validateTheme } from '../presets.ts'
  import Button from './Button.svelte'
  import Card from './Card.svelte'
  import Pill from './Pill.svelte'
  import Row from './Row.svelte'
  import Segmented from './Segmented.svelte'
  import Stat from './Stat.svelte'

  const DRAFT = '__draft'
  const clone = (t: ThemeDef): ThemeDef => JSON.parse(JSON.stringify(t))

  const start = clone(ui.current)
  if (ui.theme in tokens.themes) start.label = `My ${start.label}`
  let draft = $state(start)
  let editMode = $state<Mode>(ui.resolvedMode)
  let message = $state('')
  let pasted = $state('')

  // A mode the base theme lacks starts as a copy of the other one.
  $effect(() => {
    if (!draft.modes[editMode]) draft.modes[editMode] = { ...(draft.modes.light ?? draft.modes.dark)! }
  })

  const colours = $derived(draft.modes[editMode] ?? draft.modes[draft.defaultMode]!)
  const failures = $derived(check(colours, tokens.contrast.pairs))

  $effect(() => {
    let el = document.getElementById('ui-draft-theme')
    if (!el) {
      el = document.createElement('style')
      el.id = 'ui-draft-theme'
      document.head.append(el)
    }
    el.textContent = cssBlock(themeSelector(DRAFT, editMode), themeVars(tokens.fonts, $state.snapshot(draft) as ThemeDef, editMode))
    return () => el?.remove()
  })

  function setRadius(r: number) {
    draft.shape = { radius: r, radiusSmall: Math.round(r * 0.65), control: Math.round(r * 0.7) }
  }

  const slug = (s: string) => 'my-' + (s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'theme')

  function save() {
    const checked = validateTheme($state.snapshot(draft))
    if (typeof checked === 'string') { message = checked; return }
    const id = slug(checked.label)
    ui.saveCustom(id, checked)
    ui.set({ theme: id })
    message = `Saved and switched to "${checked.label}".`
  }

  async function copyJson() {
    try {
      await navigator.clipboard.writeText(JSON.stringify($state.snapshot(draft), null, 2))
      message = 'Theme copied. Paste it anywhere to keep or share it.'
    } catch {
      message = 'Could not reach the clipboard.'
    }
  }

  function importJson() {
    let parsed: unknown
    try { parsed = JSON.parse(pasted) } catch { message = 'That text is not a theme (not valid JSON).'; return }
    const checked = validateTheme(parsed)
    if (typeof checked === 'string') { message = checked; return }
    draft = checked
    pasted = ''
    message = `Loaded "${checked.label}". Save it to use it.`
  }

  const modeOptions: { value: Mode; label: string }[] = [{ value: 'light', label: 'Light colours' }, { value: 'dark', label: 'Dark colours' }]
</script>

<div class="editor">
  <div class="controls">
    <label class="field">Name <input bind:value={draft.label} maxlength="40" /></label>
    <label class="field">Start from
      <select onchange={(e) => { const id = (e.currentTarget as HTMLSelectElement).value; const keep = draft.label; draft = clone(ui.themes.find(([t]) => t === id)![1]); draft.label = keep }}>
        <option value="" selected disabled>Pick a theme</option>
        {#each ui.themes as [id, t] (id)}<option value={id}>{t.label}</option>{/each}
      </select>
    </label>

    <h3>Shapes and type</h3>
    <label class="field">Corner roundness <span class="hint">{draft.shape.radius}px</span>
      <input type="range" min="0" max="28" value={draft.shape.radius} oninput={(e) => setRadius(+(e.currentTarget as HTMLInputElement).value)} />
    </label>
    {#each [['body', 'Text font'], ['heading', 'Heading font'], ['number', 'Number font']] as [role, label] (role)}
      <label class="field">{label}
        <select bind:value={draft.type[role as 'body' | 'heading' | 'number']}>
          {#each Object.entries(tokens.fonts) as [id, f] (id)}<option value={id}>{f.label}</option>{/each}
        </select>
      </label>
    {/each}

    <h3>Look</h3>
    {#each Object.entries(styleGroups) as [g, info] (g)}
      <label class="field">{info.label} <span class="hint">{info.help}</span>
        <select value={presetOf(draft.style, g) ?? ''} onchange={(e) => (draft.style = applyPreset(draft.style, g, (e.currentTarget as HTMLSelectElement).value))}>
          {#each presetChoices as [id, label] (id)}<option value={id}>Like {label}</option>{/each}
        </select>
      </label>
    {/each}

    <h3>Colours</h3>
    <Segmented label="Which colours to edit" options={modeOptions} value={editMode} onchange={(m) => (editMode = m)} />
    <div class="colours">
      {#each colourNames as n (n)}
        <label class="colour">
          <input type="color" bind:value={draft.modes[editMode]![n]} />
          <span>{colourHelp[n]}</span>
        </label>
      {/each}
    </div>
  </div>

  <div class="side">
    <div class="preview" data-theme={DRAFT} data-mode={editMode}>
      <Card title="Today" explain="A preview of your theme. Nothing here is real data.">
        <div class="rows">
          <Row lead="08:00" title="Morning tablet" sub="With food">{#snippet trailing()}<Pill tone="done">Taken</Pill>{/snippet}</Row>
          <Row lead="18:30" title="Strength, 20 min" sub="Back-friendly">{#snippet trailing()}<Pill>Start</Pill>{/snippet}</Row>
        </div>
      </Card>
      <div class="stats">
        <Stat label="HbA1c" value="52" unit="mmol/mol" note="↓ 6" tone="ok" trend={[58, 57, 55, 52]} range={[48, 59]} />
        <Stat label="Home BP" value="132/84" note="7 days" tone="warn" trend={[134, 131, 136, 132]} />
      </div>
      <div class="pills"><Pill tone="ok">Good</Pill><Pill tone="warn">Worth a look</Pill><Pill tone="danger">Needs attention</Pill></div>
      <div class="pills"><Button>Main action</Button><Button variant="quiet">Second</Button><Button variant="ghost">Cancel</Button></div>
    </div>

    <div class="check" aria-live="polite">
      {#if failures.length === 0}
        <p class="good">All text is easy to read in these colours.</p>
      {:else}
        <p class="bad">Hard to read in these colours:</p>
        <ul>{#each failures as f (f.fg + f.bg)}<li>{pairWords(f.fg, f.bg)}: {f.got}:1, needs {f.need}:1</li>{/each}</ul>
      {/if}
    </div>

    <div class="actions">
      <Button onclick={save}>{failures.length ? 'Save anyway' : 'Save and use'}</Button>
      <Button variant="ghost" onclick={copyJson}>Copy as text</Button>
    </div>
    <label class="field">Load a theme someone shared <textarea bind:value={pasted} rows="3" placeholder="Paste theme text here"></textarea></label>
    <Button variant="quiet" disabled={!pasted.trim()} onclick={importJson}>Load</Button>
    {#if message}<p class="msg" role="status">{message}</p>{/if}
  </div>
</div>

<style>
  .editor { display: grid; grid-template-columns: minmax(280px, 1fr) minmax(300px, 1.1fr); gap: var(--space-xxl); align-items: start; }
  @media (max-width: 960px) { .editor { grid-template-columns: 1fr; } }
  .controls, .side { display: flex; flex-direction: column; gap: var(--space-md); }
  .side { position: sticky; top: 20px; }
  h3 { font-size: var(--text-lead); margin-top: var(--space-md); }
  .field { display: flex; flex-direction: column; gap: 4px; font-size: var(--text-small); font-weight: 600; }
  .hint { font-weight: 400; color: var(--c-muted); font-size: var(--text-caption); }
  input:not([type]), select, textarea {
    background: var(--c-panel); border: 1px solid var(--c-line); border-radius: var(--r-control); padding: 8px 10px; font-weight: 400;
  }
  input[type='range'] { accent-color: var(--c-accent); }
  .colours { display: grid; grid-template-columns: repeat(auto-fill, minmax(210px, 1fr)); gap: 6px; }
  .colour { display: flex; align-items: center; gap: 8px; font-size: var(--text-caption); }
  .colour input { width: 34px; height: 26px; border: 1px solid var(--c-line); border-radius: 6px; background: none; padding: 1px; flex: 0 0 auto; }
  .preview { background: var(--page-bg); color: var(--c-ink); font-family: var(--font-body); border-radius: var(--r); padding: var(--space-lg); display: flex; flex-direction: column; gap: var(--space-md); border: 1px solid var(--c-line); }
  .rows { display: flex; flex-direction: column; gap: 8px; }
  .stats { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .pills { display: flex; flex-wrap: wrap; gap: 8px; }
  .check { font-size: var(--text-small); }
  .good { color: var(--c-ok); font-weight: 600; }
  .bad { color: var(--c-danger); font-weight: 600; }
  ul { padding-left: 18px; color: var(--c-muted); }
  .actions { display: flex; gap: 8px; }
  .msg { font-size: var(--text-small); color: var(--c-muted); }
</style>

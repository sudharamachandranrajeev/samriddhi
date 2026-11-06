<script lang="ts">
  // The risk questionnaire: attitude (6) and capacity for loss (4), kept
  // apart, each question with its reason. The result is shown in plain words.
  import { Button } from '@samriddhi/ui'
  import { call, type RiskAnswer, type RiskProfile, type RiskQuestion } from '../lib/api.ts'
  import { answer, checkRisk, riskMissing } from '../lib/wizard.ts'
  import { toast } from '../lib/toast.svelte.ts'
  import RiskResult from './RiskResult.svelte'

  let { saveLabel, onsaved }: { saveLabel: string; onsaved: (r: RiskProfile | null) => void } = $props()

  let questions = $state<RiskQuestion[]>([])
  let answers = $state<RiskAnswer[]>([])
  let error = $state('')
  let busy = $state(false)
  let current = $state<RiskProfile | null>(null)
  const left = $derived(riskMissing(questions, answers))

  call('risk_questions').then((q) => (questions = q)).catch(toast.error.bind(toast))
  call('risk_result').then((r) => (current = r)).catch(() => {})

  const parts = [
    { part: 'attitude', title: 'How you feel about risk', note: 'Six questions about the ups and downs you are willing to live with.' },
    { part: 'capacity', title: 'What you could afford to lose', note: 'Four questions about how big a fall you could absorb without your life changing.' },
  ] as const

  async function save() {
    error = checkRisk(questions, answers) ?? ''
    if (error) return
    busy = true
    try {
      onsaved(await call('save_risk_answers', { answers: $state.snapshot(answers) }))
    } catch (e) {
      toast.error(e)
    } finally {
      busy = false
    }
  }
</script>

<div class="stack">
  {#if current}
    <div class="now"><span class="muted small">Your answers now:</span> <RiskResult result={current} compact /></div>
  {/if}
  {#each parts as p (p.part)}
    <section class="part" aria-labelledby={`part-${p.part}`}>
      <h2 id={`part-${p.part}`}>{p.title}</h2>
      <p class="muted small">{p.note} {left[p.part] ? `${left[p.part]} left.` : 'All answered.'}</p>
      {#each questions.filter((q) => q.part === p.part) as q, i (q.id)}
        <fieldset class="q">
          <legend>{i + 1}. {q.text}</legend>
          <p class="why-q">Why we ask: {q.why}</p>
          <div class="opts">
            {#each q.options as opt (opt.id)}
              <label class="choice">
                <input
                  type="radio"
                  name={q.id}
                  value={opt.id}
                  checked={answers.some((a) => a.questionId === q.id && a.optionId === opt.id)}
                  onchange={() => (answers = answer(answers, q.id, opt.id))}
                />
                {opt.text}
              </label>
            {/each}
          </div>
        </fieldset>
      {/each}
    </section>
  {/each}

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="row-actions"><Button onclick={save} disabled={busy}>{saveLabel}</Button></div>
</div>

<style>
  .part { display: flex; flex-direction: column; gap: var(--space-md); }
  .part h2 { font-size: var(--text-title); margin-top: var(--space-md); }
  .q { border-top: 1px solid var(--c-line); padding-top: var(--space-md); }
  .why-q { color: var(--c-muted); font-size: var(--text-caption); margin: -4px 0 var(--space-sm); }
  .opts { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: var(--space-sm); }
  .now { padding: var(--space-md) var(--space-lg); border-radius: var(--r-sm); background: var(--c-panel-2); }
</style>

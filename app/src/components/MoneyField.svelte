<script lang="ts">
  // An amount box: text in, exact minor units out (checked as you type).
  import { formatMoney, parseAmount } from '../lib/money.ts'

  let {
    id,
    label,
    help = '',
    currency,
    value = $bindable(''),
  }: { id: string; label: string; help?: string; currency: string; value?: string } = $props()

  const parsed = $derived(value.trim() ? parseAmount(value, currency) : null)
  const error = $derived(parsed && !parsed.ok ? parsed.error : '')
</script>

<div class="field">
  <label for={id}>{label}</label>
  <div class="box">
    <span class="cur" aria-hidden="true">{currency}</span>
    <input
      {id}
      class="input num"
      inputmode="decimal"
      autocomplete="off"
      bind:value
      aria-invalid={error ? 'true' : undefined}
      aria-describedby={`${id}-help`}
    />
  </div>
  <span class="help" id={`${id}-help`} aria-live="polite">
    {#if error}<span class="error">{error}</span>{:else if parsed?.ok}{formatMoney(parsed.money)}{#if help}{` · ${help}`}{/if}{:else}{help}{/if}
  </span>
</div>

<style>
  .box { position: relative; display: flex; }
  .cur { position: absolute; left: 12px; top: 50%; translate: 0 -50%; font-size: var(--text-caption); font-weight: 600; color: var(--c-muted); pointer-events: none; }
  .box .input { padding-left: 52px; }
</style>

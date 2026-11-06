<script lang="ts">
  // The "?" next to every number and screen. Opens a short plain-language note.
  let { title, text }: { title: string; text: string } = $props()
  const id = `explain-${Math.random().toString(36).slice(2, 9)}`
  let button: HTMLButtonElement
  let panel: HTMLDivElement

  // Popovers sit in the top layer, so place this one next to its button when it opens.
  function place(e: Event) {
    if ((e as ToggleEvent).newState !== 'open') return
    const b = button.getBoundingClientRect()
    const w = Math.min(300, innerWidth - 24)
    panel.style.width = `${w}px`
    panel.style.left = `${Math.max(12, Math.min(b.left, innerWidth - w - 12))}px`
    const below = b.bottom + 8
    const h = panel.offsetHeight
    panel.style.top = `${below + h > innerHeight - 12 ? Math.max(12, b.top - h - 8) : below}px`
  }
</script>

<button bind:this={button} class="q" popovertarget={id} aria-label={`What is ${title}?`}>?</button>
<div bind:this={panel} {id} popover class="panel" role="note" ontoggle={place}>
  <strong>{title}</strong>
  <p>{text}</p>
</div>

<style>
  .q {
    width: 18px; height: 18px; border-radius: 50%; flex: 0 0 auto;
    border: 1.4px solid var(--c-muted); background: none; color: var(--c-muted);
    font-size: 10px; font-weight: 700; line-height: 1; cursor: help;
    display: inline-grid; place-items: center; text-transform: none; letter-spacing: 0;
    transition: color var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease);
  }
  .q:hover { color: var(--c-accent); border-color: var(--c-accent); }
  .panel {
    position: fixed; inset: auto; margin: 0;
    background: var(--c-panel); color: var(--c-ink); border: 1px solid var(--c-line);
    border-radius: var(--r-sm); padding: var(--space-md) var(--space-lg);
    box-shadow: 0 12px 32px rgb(0 0 0 / .18); font-size: var(--text-small);
    text-transform: none; letter-spacing: 0; font-weight: 400; font-family: var(--font-body);
    opacity: 0; translate: 0 4px;
    transition: opacity var(--dur-base) var(--ease), translate var(--dur-base) var(--ease), display var(--dur-base) allow-discrete, overlay var(--dur-base) allow-discrete;
  }
  .panel:popover-open { opacity: 1; translate: 0 0; }
  @starting-style { .panel:popover-open { opacity: 0; translate: 0 4px; } }
  strong { display: block; margin-bottom: var(--space-xs); font-weight: 600; }
  p { color: var(--c-muted); line-height: 1.5; }
</style>

<script lang="ts">
  // Every theme as a live miniature. Each card renders in its own theme, whatever the app uses.
  import { ui } from '../theme.svelte.ts'
  import Segmented from './Segmented.svelte'
  import type { ModePref, MotionPref } from '../theme.svelte.ts'

  const modes: { value: ModePref; label: string }[] = [
    { value: 'system', label: 'Match device' }, { value: 'light', label: 'Light' }, { value: 'dark', label: 'Dark' },
  ]
  const motions: { value: MotionPref; label: string }[] = [
    { value: 'system', label: 'Match device' }, { value: 'calm', label: 'Calm' }, { value: 'full', label: 'Full motion' },
  ]
</script>

<div class="controls">
  <div><span class="lbl">Light or dark</span><Segmented label="Light or dark" options={modes} value={ui.mode} onchange={(mode) => ui.set({ mode })} /></div>
  <div><span class="lbl">Motion</span><Segmented label="Motion" options={motions} value={ui.motion} onchange={(motion) => ui.set({ motion })} /></div>
</div>

<div class="grid" role="radiogroup" aria-label="Theme">
  {#each ui.themes as [id, t] (id)}
    <button
      role="radio"
      aria-checked={ui.theme === id}
      class="pick"
      class:on={ui.theme === id}
      onclick={() => ui.set({ theme: id })}
    >
      <div class="mini" data-theme={id} data-mode={t.modes[ui.resolvedMode] ? ui.resolvedMode : t.defaultMode}>
        <div class="m-rail"><i></i><b class="m-on"></b><b></b><b></b></div>
        <div class="m-main">
          <div class="m-card"><span class="m-title">Today</span><span class="m-row"></span><span class="m-row"></span></div>
          <div class="m-card"><span class="m-num">52</span><span class="m-pill"></span></div>
        </div>
      </div>
      <span class="name">{t.label}{#if ui.theme === id}<span class="tick" aria-hidden="true">✓</span>{/if}</span>
      <span class="desc">{t.description}</span>
    </button>
  {/each}
</div>

<style>
  .controls { display: flex; flex-wrap: wrap; gap: var(--space-xl); margin-bottom: var(--space-xl); }
  .controls > div { display: flex; flex-direction: column; gap: 6px; }
  .lbl { font-size: var(--text-caption); color: var(--c-muted); font-weight: 600; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(230px, 1fr)); gap: var(--space-lg); }
  .pick {
    text-align: left; background: var(--c-panel); border: 2px solid var(--c-line); border-radius: var(--r); padding: 10px; cursor: pointer;
    display: flex; flex-direction: column; gap: 6px;
    transition: border-color var(--dur-base) var(--ease), transform var(--dur-base) var(--spring);
  }
  .pick:hover { transform: translateY(-2px); }
  .pick.on { border-color: var(--c-accent); }
  .name { font-weight: 600; display: flex; align-items: center; gap: 6px; padding: 0 4px; }
  .tick { color: var(--c-accent); }
  .desc { font-size: var(--text-caption); color: var(--c-muted); padding: 0 4px 4px; }

  .mini { height: 120px; border-radius: calc(var(--r-sm)); overflow: hidden; display: flex; background: var(--page-bg); color: var(--c-ink); font-family: var(--font-body); }
  .m-rail { width: 46px; padding: 8px 6px; display: flex; flex-direction: column; gap: 5px; border-right: 1px solid var(--c-line); }
  .m-rail i { width: 14px; height: 14px; border-radius: 5px; background: linear-gradient(135deg, var(--c-accent), var(--c-accent-2)); margin-bottom: 4px; }
  .m-rail b { height: 7px; border-radius: 4px; background: var(--c-line); }
  .m-rail b.m-on { background: var(--nav-on-bg); border-left: var(--nav-on-bar) solid var(--c-accent); }
  .m-main { flex: 1; display: grid; grid-template-columns: 1.4fr 1fr; gap: 6px; padding: 8px; }
  .m-card { background: var(--card-bg); border: var(--card-border); box-shadow: var(--card-shadow); border-radius: calc(var(--r) * .5); padding: 6px; display: flex; flex-direction: column; gap: 5px; }
  .m-title { font-family: var(--title-font); text-transform: var(--title-case); font-size: 9px; color: var(--title-color); font-weight: 600; letter-spacing: var(--title-tracking); }
  .m-row { height: 14px; background: var(--row-bg); border-radius: calc(var(--row-radius) * .5); border-bottom: var(--row-divider); }
  .m-num { font-family: var(--font-number); font-size: 22px; font-weight: var(--weight-display); }
  .m-pill { width: 34px; height: 10px; border-radius: 9px; background: var(--c-accent-soft); }
</style>

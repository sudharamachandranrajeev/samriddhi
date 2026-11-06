<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { statusText, type Status } from './lib/status.ts'

  let status: Status | null = $state(null)
  let error = $state('')

  invoke<Status>('status')
    .then((s) => (status = s))
    .catch((e) => (error = String(e)))
</script>

<main>
  <h1>Samriddhi</h1>
  <p>{error || statusText(status)}</p>
</main>

<style>
  main {
    min-height: 100vh;
    display: grid;
    place-content: center;
    text-align: center;
    padding: 1rem;
  }
</style>

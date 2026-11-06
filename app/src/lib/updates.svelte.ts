// Update state shared by the start-up banner and Settings > Updates. The app
// checks once at start (unless turned off) and never installs without the
// user pressing the button. "Later" always works. Ported from Arogya Swasthya.
import { call, onUpdateProgress, type InstallKind, type UpdateInfo, type UpdateStatus } from './api.ts'
import { toast } from './toast.svelte.ts'

export const kindName: Record<InstallKind, string> = {
  installer: 'installed with the Windows installer',
  appImage: 'the AppImage',
  pacman: 'the Arch Linux package',
  portable: 'the portable version for Windows',
  android: 'the Android app',
  other: 'a copy built from source or installed another way',
}

/** Bytes as a short size: "9.0 MB". */
export function sizeText(bytes: number): string {
  return bytes < 1_048_576 ? `${Math.max(1, Math.round(bytes / 1024))} KB` : `${(bytes / 1_048_576).toFixed(1)} MB`
}

class Updates {
  status = $state<UpdateStatus | null>(null)
  found = $state<UpdateInfo | null>(null)
  busy = $state(false)
  /** 0 to 1 while downloading, null otherwise. */
  progress = $state<number | null>(null)
  /** Total download size once the download starts, when the list did not give it. */
  total = $state<number | null>(null)
  /** The banner was put away for this run ("Later"). */
  dismissed = $state(false)
  #started = false
  #listening = false

  async load() {
    this.status = await call('update_status')
  }

  /** Once, after unlocking: check quietly unless updates are off. Offline is not an error here. */
  async startup() {
    if (this.#started) return
    this.#started = true
    try {
      await this.load()
      if (this.status?.mode === 'ask') await this.check(true)
    } catch {
      // Status could not be read: Settings shows the problem.
    }
  }

  async check(quiet = false) {
    try {
      this.found = await call('check_update')
      if (!quiet && !this.found) toast.show('Samriddhi is up to date.')
    } catch (e) {
      if (!quiet) toast.error(e)
    }
  }

  async #listen() {
    if (this.#listening) return
    this.#listening = true
    await onUpdateProgress((done, total) => {
      this.total = total
      this.progress = total ? done / total : null
    })
  }

  /** Back up, download, check the signature, install and restart. */
  async install() {
    const found = this.found
    if (!found) return
    await this.#run(() => call('install_update', { version: found.version }))
    if (this.status?.kind === 'android') toast.show('Android is asking you to confirm the update.')
  }

  async rollback(restore: boolean) {
    await this.#run(() => call('rollback', { restore }))
  }

  async #run(f: () => Promise<unknown>) {
    this.busy = true
    await this.#listen()
    try {
      await f()
    } catch (e) {
      toast.error(e)
    } finally {
      this.busy = false
      this.progress = null
    }
  }
}

export const updates = new Updates()

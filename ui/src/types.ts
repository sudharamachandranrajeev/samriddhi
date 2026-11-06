// Shared component types.

import type { Component } from 'svelte'

/** `icon` is an icon component (for example from @lucide/svelte), drawn the same on every device. */
export interface NavItem { id: string; label: string; icon: Component<{ size?: number | string }> }

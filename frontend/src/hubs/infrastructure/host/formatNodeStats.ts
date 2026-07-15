/** Proxmox node status ("online"/"offline"/"unknown") doesn't map onto
 * StatusPill's resource-phase variants, so nodes get their own small
 * mapping onto the same `.az-pill` CSS classes. */
export function nodeStatusVariant(status: string): 'success' | 'danger' | 'default' {
  if (status === 'online') return 'success'
  if (status === 'offline') return 'danger'
  return 'default'
}

/** Proxmox reports cpu as a 0..1 fraction of maxcpu, not a raw percentage. */
export function formatCpu(cpu: number | null, maxCpu: number | null): string {
  if (cpu === null) return '—'
  const pct = (cpu * 100).toFixed(1)
  return maxCpu !== null ? `${pct}% of ${maxCpu} cores` : `${pct}%`
}

export function formatBytes(bytes: number | null): string {
  if (bytes === null) return '—'
  const gib = bytes / 1024 ** 3
  return `${gib.toFixed(1)} GiB`
}

export function formatMemory(mem: number | null, maxMem: number | null): string {
  if (mem === null || maxMem === null) return '—'
  return `${formatBytes(mem)} / ${formatBytes(maxMem)}`
}

export function formatUptime(seconds: number | null): string {
  if (seconds === null) return '—'
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  if (days > 0) return `${days}d ${hours}h`
  const minutes = Math.floor((seconds % 3600) / 60)
  if (hours > 0) return `${hours}h ${minutes}m`
  return `${minutes}m`
}

const PHASE_VARIANT: Record<string, 'success' | 'warning' | 'danger'> = {
  Ready: 'success',
  Pending: 'warning',
  ProvisioningInfra: 'warning',
  Bootstrapping: 'warning',
  HealthChecking: 'warning',
  Scaling: 'warning',
  Upgrading: 'warning',
  Deleting: 'warning',
  Failed: 'danger',
  Degraded: 'danger',
  NotReady: 'danger',
}

function variantFor(phase: string): 'success' | 'warning' | 'danger' | 'default' {
  const key = Object.keys(PHASE_VARIANT).find((k) => phase.startsWith(k))
  return key ? PHASE_VARIANT[key] : 'default'
}

export function StatusPill({ phase }: { phase: string }) {
  return <span className={`az-pill az-pill-${variantFor(phase)}`}>{phase}</span>
}

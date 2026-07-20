import { useOutletContext } from 'react-router-dom'
import type { ProviderDetail } from '../../../api/providers'
import { EssentialsGrid, type EssentialItem } from '../../../ui/EssentialsGrid'

export function SettingsTab() {
  const host = useOutletContext<ProviderDetail>()

  const items: EssentialItem[] = [
    { label: 'Name', value: host.name },
    { label: 'URL', value: host.config.url },
    { label: 'Token ID', value: host.config.token_id },
    { label: 'Token Secret', value: host.config.token_secret },
    { label: 'Allow insecure TLS', value: host.config.tls_insecure ? 'Yes' : 'No' },
    {
      label: 'Hardware virtualization (KVM)',
      value: host.config.kvm === false ? 'Off — VMs use software emulation' : 'On',
    },
    {
      label: 'SSH (snippets)',
      value: host.config.ssh_private_key
        ? `${host.config.ssh_user ?? 'root'}@host:${host.config.ssh_port ?? 22}`
        : 'Not configured — Kubernetes clusters and custom cloud-init will fail',
    },
    {
      label: 'VM debug SSH key',
      value: host.config.ssh_public_key ?? 'Not configured',
    },
  ]

  return (
    <div className="az-stack-col az-gap-4">
      <h2>Settings</h2>
      <p className="az-text-secondary">
        Read-only for now — editing connection details after creation isn&apos;t available yet
        (see issue #32). Delete and re-add the host to change these.
      </p>
      <EssentialsGrid items={items} />
    </div>
  )
}

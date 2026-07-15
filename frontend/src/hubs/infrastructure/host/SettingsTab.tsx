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

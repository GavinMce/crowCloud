import { Link } from 'react-router-dom'
import { getHub } from '../hubConfig'
import { useProviders } from '../../api/providers'
import { Button } from '../../ui/Button'

export function InfrastructureOverviewPage() {
  const hub = getHub('infrastructure')
  const providers = useProviders()
  if (!hub) return null

  const proxmoxCount = (providers.data ?? []).filter((p) => p.provider_type === 'proxmox').length

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <div>
          <h1>{hub.label}</h1>
          <p className="az-text-secondary">{hub.description}</p>
        </div>

        <div className="az-tile-grid">
          {hub.resourceTypes.map((rt) => (
            <div key={rt.id} className="az-card">
              <h2 style={{ margin: 0 }}>{rt.label}</h2>
              <p className="az-text-secondary" style={{ margin: '8px 0' }}>
                {rt.description}
              </p>
              <p style={{ margin: '8px 0', fontSize: 20, fontWeight: 600 }}>
                {rt.status === 'placeholder' ? 'Not available yet' : proxmoxCount}
              </p>
              <div className="az-stack-row az-gap-2">
                <Link to={`/infrastructure/${rt.id}`}>
                  <Button variant="default" size="sm">
                    View
                  </Button>
                </Link>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

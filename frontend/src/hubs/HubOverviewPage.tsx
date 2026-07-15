import { Link } from 'react-router-dom'
import { getHub } from './hubConfig'
import { useCurrentProject } from '../hooks/useCurrentProject'
import { useResources } from '../api/resources'
import { Button } from '../ui/Button'

export function HubOverviewPage({ hubId }: { hubId: string }) {
  const hub = getHub(hubId)
  const { current } = useCurrentProject()
  const resources = useResources(current ?? '')

  if (!hub) return null

  const hasLiveTypes = hub.resourceTypes.some((rt) => rt.status === 'live')

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <div>
          <h1>{hub.label}</h1>
          <p className="az-text-secondary">{hub.description}</p>
        </div>

        {hasLiveTypes && !current && (
          <p className="az-text-secondary">
            Select or create a project from the top bar to see {hub.label.toLowerCase()}{' '}
            resources.
          </p>
        )}

        <div className="az-tile-grid">
          {hub.resourceTypes.map((rt) => {
            const count =
              rt.status === 'live' && current
                ? (resources.data ?? []).filter((r) => r.resource_type === rt.apiResourceType)
                    .length
                : null

            return (
              <div key={rt.id} className="az-card">
                <div className="az-stack-row az-justify-between az-gap-2">
                  <h2 style={{ margin: 0 }}>{rt.label}</h2>
                </div>
                <p className="az-text-secondary" style={{ margin: '8px 0' }}>
                  {rt.description}
                </p>
                <p style={{ margin: '8px 0', fontSize: 20, fontWeight: 600 }}>
                  {rt.status === 'placeholder' ? 'Not available yet' : (count ?? '—')}
                </p>
                <div className="az-stack-row az-gap-2">
                  <Link to={`/${hub.id}/${rt.id}`}>
                    <Button variant="default" size="sm">
                      View
                    </Button>
                  </Link>
                  {rt.status === 'live' && current && (
                    <Link to={`/${hub.id}/${rt.id}/create`}>
                      <Button variant="primary" size="sm">
                        + Create
                      </Button>
                    </Link>
                  )}
                </div>
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}

import { HUBS } from '../hubs/hubConfig'
import { useCurrentProject } from '../hooks/useCurrentProject'
import { useProviders } from '../api/providers'
import { ManagementIcon } from '../ui/icons'
import { ServiceTile } from '../ui/ServiceTile'

export function HomePage() {
  const { projectNames, current } = useCurrentProject()
  const providers = useProviders()

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-6">
        <div>
          <h1>crowCloud</h1>
          <p className="az-text-secondary">
            {current
              ? `Current project: ${current}`
              : 'Select or create a project from the top bar to get started.'}
          </p>
        </div>

        <div className="az-stack-row az-gap-6">
          <div>
            <div style={{ fontSize: 24, fontWeight: 600 }}>{projectNames.length}</div>
            <div className="az-text-secondary">Projects</div>
          </div>
          <div>
            <div style={{ fontSize: 24, fontWeight: 600 }}>{providers.data?.length ?? '—'}</div>
            <div className="az-text-secondary">Cloud hosts</div>
          </div>
        </div>

        <section>
          <h2>Services</h2>
          <div className="az-tile-grid">
            {HUBS.map((hub) => (
              <ServiceTile
                key={hub.id}
                icon={<hub.icon size={24} />}
                title={hub.label}
                description={hub.description}
                to={`/${hub.id}`}
              />
            ))}
            <ServiceTile
              icon={<ManagementIcon size={24} />}
              title="Management"
              description="Projects and cloud hosts."
              to="/management/projects"
            />
          </div>
        </section>
      </div>
    </div>
  )
}

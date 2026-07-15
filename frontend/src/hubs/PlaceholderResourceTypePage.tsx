import { getHub } from './hubConfig'
import { Button } from '../ui/Button'
import { CommandBar } from '../ui/CommandBar'

export function PlaceholderResourceTypePage({
  hubId,
  typeId,
}: {
  hubId: string
  typeId: string
}) {
  const hub = getHub(hubId)
  const resourceType = hub?.resourceTypes.find((rt) => rt.id === typeId)
  if (!hub || !resourceType) return null

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <h1>{resourceType.label}</h1>
        <CommandBar>
          <Button variant="primary" disabled>
            + Create {resourceType.label}
          </Button>
        </CommandBar>
        <div className="az-placeholder">
          <p style={{ margin: 0, fontWeight: 600 }}>Not available yet</p>
          <p style={{ margin: '8px 0 0' }}>{resourceType.description}</p>
          <p className="az-text-secondary" style={{ margin: '8px 0 0' }}>
            crowCloud doesn't have a working API for this resource type yet.
          </p>
        </div>
      </div>
    </div>
  )
}

import { Link } from 'react-router-dom'
import { getHub } from './hubConfig'
import { useCurrentProject } from '../hooks/useCurrentProject'
import { useResources } from '../api/resources'
import type { ResourceRow } from '../api/resources'
import { DataTable, type DataTableColumn } from '../ui/DataTable'
import { StatusPill } from '../ui/StatusPill'

export function AllResourcesPage({ hubId }: { hubId: string }) {
  const hub = getHub(hubId)
  const { current } = useCurrentProject()
  const resources = useResources(current ?? '')

  if (!hub) return null

  const liveTypes = new Set(
    hub.resourceTypes.filter((rt) => rt.status === 'live').map((rt) => rt.apiResourceType),
  )
  const rows = (resources.data ?? []).filter((r) => liveTypes.has(r.resource_type))

  const columns: DataTableColumn<ResourceRow>[] = [
    {
      key: 'name',
      header: 'Name',
      render: (row) => (
        <Link to={`/${hub.id}/${resourceTypePath(hub.id, row.resource_type)}/${encodeURIComponent(row.name)}`}>
          {row.name}
        </Link>
      ),
    },
    { key: 'resource_type', header: 'Type' },
    { key: 'phase', header: 'Status', render: (row) => <StatusPill phase={row.phase} /> },
    {
      key: 'created_at',
      header: 'Created',
      render: (row) => new Date(row.created_at).toLocaleString(),
    },
  ]

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <h1>All resources</h1>
        {liveTypes.size === 0 && (
          <div className="az-placeholder">
            {hub.label} doesn't have any resource types available yet.
          </div>
        )}
        {liveTypes.size > 0 && !current && (
          <p className="az-text-secondary">Select or create a project from the top bar first.</p>
        )}
        {liveTypes.size > 0 && current && rows.length === 0 && <p>No resources yet.</p>}
        {liveTypes.size > 0 && current && rows.length > 0 && (
          <DataTable columns={columns} data={rows} keyField="id" />
        )}
      </div>
    </div>
  )
}

/** Compute hub's `vm` resource_type lives at the `virtual-machines` path. */
function resourceTypePath(hubId: string, apiResourceType: string): string {
  const hub = getHub(hubId)
  const rt = hub?.resourceTypes.find((r) => r.apiResourceType === apiResourceType)
  return rt?.id ?? apiResourceType
}

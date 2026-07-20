import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useCurrentProject } from '../../hooks/useCurrentProject'
import { useDeleteResource, useResources } from '../../api/resources'
import type { ResourceRow } from '../../api/resources'
import { Button } from '../../ui/Button'
import { CommandBar } from '../../ui/CommandBar'
import { DataTable, type DataTableColumn } from '../../ui/DataTable'
import { Modal } from '../../ui/Modal'
import { StatusPill } from '../../ui/StatusPill'

export function KubernetesClustersPage() {
  const { current } = useCurrentProject()
  const resources = useResources(current ?? '')
  const deleteResource = useDeleteResource(current ?? '')
  const [pendingDelete, setPendingDelete] = useState<string | null>(null)

  const clusters = (resources.data ?? []).filter((r) => r.resource_type === 'k8s_cluster')

  const handleDelete = async () => {
    if (!pendingDelete) return
    await deleteResource.mutateAsync(pendingDelete)
    setPendingDelete(null)
  }

  const columns: DataTableColumn<ResourceRow>[] = [
    {
      key: 'name',
      header: 'Name',
      render: (row) => (
        <Link to={`/containers/kubernetes-clusters/${encodeURIComponent(row.name)}`}>
          {row.name}
        </Link>
      ),
    },
    { key: 'phase', header: 'Status', render: (row) => <StatusPill phase={row.phase} /> },
    {
      key: 'created_at',
      header: 'Created',
      render: (row) => new Date(row.created_at).toLocaleString(),
    },
    {
      key: 'id',
      header: '',
      render: (row) => (
        <Button variant="ghost" size="sm" onClick={() => setPendingDelete(row.name)}>
          Delete
        </Button>
      ),
    },
  ]

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <h1>Kubernetes clusters</h1>
        <CommandBar>
          <Link to="/containers/kubernetes-clusters/create">
            <Button variant="primary" disabled={!current}>
              + Create
            </Button>
          </Link>
        </CommandBar>

        {!current && (
          <p className="az-text-secondary">
            Select or create a project from the top bar to see Kubernetes clusters.
          </p>
        )}
        {current && resources.isLoading && <p>Loading…</p>}
        {current && resources.isError && (
          <p className="az-alert az-alert-danger">Failed to load Kubernetes clusters.</p>
        )}
        {current && resources.data && clusters.length === 0 && <p>No Kubernetes clusters yet.</p>}
        {current && clusters.length > 0 && (
          <DataTable columns={columns} data={clusters} keyField="id" />
        )}
      </div>

      <Modal
        open={pendingDelete !== null}
        title="Delete Kubernetes cluster"
        onClose={() => setPendingDelete(null)}
      >
        <div className="az-stack-col az-gap-4">
          <p>
            Delete Kubernetes cluster <strong>{pendingDelete}</strong>? This deletes the control
            plane and all worker VMs. This cannot be undone.
          </p>
          <div className="az-stack-row az-gap-2">
            <Button variant="primary" onClick={handleDelete} disabled={deleteResource.isPending}>
              Delete
            </Button>
            <Button variant="default" onClick={() => setPendingDelete(null)}>
              Cancel
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  )
}

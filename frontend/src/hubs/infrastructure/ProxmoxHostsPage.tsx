import { useState } from 'react'
import { Link } from 'react-router-dom'
import { type ProviderRow, useDeleteProvider, useProviders } from '../../api/providers'
import { Button } from '../../ui/Button'
import { CommandBar } from '../../ui/CommandBar'
import { DataTable, type DataTableColumn } from '../../ui/DataTable'
import { Modal } from '../../ui/Modal'
import { useAuth } from '../../auth/useAuth'

export function ProxmoxHostsPage() {
  const { isAdmin } = useAuth()
  const providers = useProviders()
  const deleteProvider = useDeleteProvider()

  const [pendingDelete, setPendingDelete] = useState<ProviderRow | null>(null)

  const hosts = (providers.data ?? []).filter((p) => p.provider_type === 'proxmox')

  const handleDelete = async () => {
    if (!pendingDelete) return
    await deleteProvider.mutateAsync(pendingDelete.id)
    setPendingDelete(null)
  }

  const columns: DataTableColumn<ProviderRow>[] = [
    {
      key: 'name',
      header: 'Name',
      render: (row) => <Link to={`/infrastructure/proxmox-hosts/${row.id}`}>{row.name}</Link>,
    },
    {
      key: 'created_at',
      header: 'Created',
      render: (row) => new Date(row.created_at).toLocaleString(),
    },
    ...(isAdmin
      ? [
          {
            key: 'id',
            header: '',
            render: (row: ProviderRow) => (
              <Button variant="ghost" size="sm" onClick={() => setPendingDelete(row)}>
                Delete
              </Button>
            ),
          },
        ]
      : []),
  ]

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <h1>Proxmox hosts</h1>
        <CommandBar>
          {isAdmin ? (
            <Link to="/infrastructure/proxmox-hosts/create">
              <Button variant="primary">+ Create</Button>
            </Link>
          ) : (
            <p className="az-text-secondary">Only admins can add Proxmox hosts.</p>
          )}
        </CommandBar>

        {providers.isLoading && <p>Loading…</p>}
        {providers.isError && (
          <p className="az-alert az-alert-danger">Failed to load Proxmox hosts.</p>
        )}
        {providers.data && hosts.length === 0 && <p>No Proxmox hosts registered yet.</p>}
        {hosts.length > 0 && <DataTable columns={columns} data={hosts} keyField="id" />}
      </div>

      <Modal
        open={pendingDelete !== null}
        title="Delete Proxmox host"
        onClose={() => setPendingDelete(null)}
      >
        <div className="az-stack-col az-gap-4">
          <p>
            Delete Proxmox host <strong>{pendingDelete?.name}</strong>? This cannot be undone.
          </p>
          <div className="az-stack-row az-gap-2">
            <Button variant="primary" onClick={handleDelete} disabled={deleteProvider.isPending}>
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

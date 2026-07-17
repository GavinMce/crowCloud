import { useState } from 'react'
import { Link } from 'react-router-dom'
import { type IpPoolRow, useDeleteIpPool, useIpPools } from '../../api/ipPools'
import { Button } from '../../ui/Button'
import { CommandBar } from '../../ui/CommandBar'
import { DataTable, type DataTableColumn } from '../../ui/DataTable'
import { Modal } from '../../ui/Modal'
import { useAuth } from '../../auth/useAuth'

function fmtCounter(v: number | null) {
  return v === null ? '—' : v.toString()
}

export function IpPoolsPage() {
  const { isAdmin } = useAuth()
  const ipPools = useIpPools()
  const deleteIpPool = useDeleteIpPool()

  const [pendingDelete, setPendingDelete] = useState<IpPoolRow | null>(null)

  const handleDelete = async () => {
    if (!pendingDelete) return
    await deleteIpPool.mutateAsync(pendingDelete.name)
    setPendingDelete(null)
  }

  const columns: DataTableColumn<IpPoolRow>[] = [
    {
      key: 'name',
      header: 'Name',
      render: (row) => <Link to={`/networking/ip-pools/${row.name}`}>{row.name}</Link>,
    },
    { key: 'cidr', header: 'CIDR' },
    {
      key: 'range_start',
      header: 'Range',
      render: (row) => `${row.range_start} – ${row.range_end}`,
    },
    { key: 'bridge', header: 'Bridge' },
    {
      key: 'allocated',
      header: 'Allocated / available',
      render: (row) => `${fmtCounter(row.allocated)} / ${fmtCounter(row.available)}`,
    },
    ...(isAdmin
      ? [
          {
            key: 'actions',
            header: '',
            render: (row: IpPoolRow) => (
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
        <h1>IP pools</h1>
        <CommandBar>
          {isAdmin ? (
            <Link to="/networking/ip-pools/create">
              <Button variant="primary">+ Create</Button>
            </Link>
          ) : (
            <p className="az-text-secondary">Only admins can add IP pools.</p>
          )}
        </CommandBar>

        {ipPools.isLoading && <p>Loading…</p>}
        {ipPools.isError && <p className="az-alert az-alert-danger">Failed to load IP pools.</p>}
        {ipPools.data && ipPools.data.length === 0 && <p>No IP pools registered yet.</p>}
        {ipPools.data && ipPools.data.length > 0 && (
          <DataTable columns={columns} data={ipPools.data} keyField="name" />
        )}
      </div>

      <Modal open={pendingDelete !== null} title="Delete IP pool" onClose={() => setPendingDelete(null)}>
        <div className="az-stack-col az-gap-4">
          <p>
            Delete IP pool <strong>{pendingDelete?.name}</strong>? This cannot be undone.
          </p>
          <div className="az-stack-row az-gap-2">
            <Button variant="primary" onClick={handleDelete} disabled={deleteIpPool.isPending}>
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

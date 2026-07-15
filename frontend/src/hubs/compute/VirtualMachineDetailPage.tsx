import { useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useCurrentProject } from '../../hooks/useCurrentProject'
import { useDeleteResource, useResource } from '../../api/resources'
import { Breadcrumb } from '../../ui/Breadcrumb'
import { Button } from '../../ui/Button'
import { CommandBar } from '../../ui/CommandBar'
import { EssentialsGrid, type EssentialItem } from '../../ui/EssentialsGrid'
import { Modal } from '../../ui/Modal'
import { StatusPill } from '../../ui/StatusPill'

function vmIp(handle: unknown): string | null {
  if (handle && typeof handle === 'object' && 'ip' in handle) {
    const ip = (handle as { ip?: unknown }).ip
    return typeof ip === 'string' ? ip : null
  }
  return null
}

export function VirtualMachineDetailPage() {
  const { name = '' } = useParams()
  const navigate = useNavigate()
  const { current } = useCurrentProject()
  const detail = useResource(current ?? '', name)
  const deleteResource = useDeleteResource(current ?? '')
  const [confirmOpen, setConfirmOpen] = useState(false)

  const handleDelete = async () => {
    await deleteResource.mutateAsync(name)
    navigate('/compute/virtual-machines')
  }

  const items: EssentialItem[] = detail.data
    ? [
        { label: 'Status', value: <StatusPill phase={detail.data.phase} /> },
        { label: 'Project', value: current },
        { label: 'IP address', value: vmIp(detail.data.handle) ?? 'Not assigned yet' },
        { label: 'Created', value: new Date(detail.data.created_at).toLocaleString() },
      ]
    : []

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <Breadcrumb
          items={[{ label: 'Virtual machines', to: '/compute/virtual-machines' }, { label: name }]}
        />
        <h1>{name}</h1>
        <CommandBar>
          <Button variant="default" onClick={() => setConfirmOpen(true)}>
            Delete
          </Button>
        </CommandBar>

        {detail.isLoading && <p>Loading…</p>}
        {detail.isError && <p className="az-alert az-alert-danger">Failed to load this virtual machine.</p>}
        {detail.data && <EssentialsGrid items={items} />}
      </div>

      <Modal open={confirmOpen} title="Delete virtual machine" onClose={() => setConfirmOpen(false)}>
        <div className="az-stack-col az-gap-4">
          <p>
            Delete virtual machine <strong>{name}</strong>? This cannot be undone.
          </p>
          <div className="az-stack-row az-gap-2">
            <Button variant="primary" onClick={handleDelete} disabled={deleteResource.isPending}>
              Delete
            </Button>
            <Button variant="default" onClick={() => setConfirmOpen(false)}>
              Cancel
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  )
}

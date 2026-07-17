import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useCurrentProject } from '../../hooks/useCurrentProject'
import { useDeleteResource, useResource, useResources } from '../../api/resources'
import { parseDiskHandle, useUpdateDisk, type UpdateDiskRequest } from '../../api/disks'
import { ApiError } from '../../api/client'
import { Button } from '../../ui/Button'
import { CommandBar } from '../../ui/CommandBar'
import { EssentialsGrid, type EssentialItem } from '../../ui/EssentialsGrid'
import { Modal } from '../../ui/Modal'
import { Select } from '../../ui/Select'
import { StatusPill } from '../../ui/StatusPill'
import { TextField } from '../../ui/TextField'

export function DiskOverviewTab() {
  const { name = '' } = useParams()
  const navigate = useNavigate()
  const { current } = useCurrentProject()
  const detail = useResource(current ?? '', name)
  const resources = useResources(current ?? '')
  const deleteResource = useDeleteResource(current ?? '')
  const updateDisk = useUpdateDisk(current ?? '')

  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false)
  const [confirmDetachOpen, setConfirmDetachOpen] = useState(false)
  const [attachOpen, setAttachOpen] = useState(false)
  const [attachVm, setAttachVm] = useState('')
  const [resizeOpen, setResizeOpen] = useState(false)
  const [resizeValue, setResizeValue] = useState(0)
  const [actionError, setActionError] = useState<string | null>(null)

  const vms = (resources.data ?? []).filter((r) => r.resource_type === 'vm')

  if (detail.isLoading) {
    return <p>Loading…</p>
  }
  if (detail.isError || !detail.data) {
    return <p className="az-alert az-alert-danger">Failed to load this disk.</p>
  }

  const data = parseDiskHandle(detail.data.handle)
  const attachedTo = data?.attached_vm_ref?.name ?? null
  const isAttached = attachedTo !== null

  const handleDelete = async () => {
    await deleteResource.mutateAsync(name)
    navigate('/compute/disks')
  }

  const runUpdate = async (req: UpdateDiskRequest) => {
    setActionError(null)
    try {
      await updateDisk.mutateAsync({ name, req })
      setAttachOpen(false)
      setConfirmDetachOpen(false)
      setResizeOpen(false)
    } catch (err) {
      setActionError(err instanceof ApiError ? err.message : 'Action failed')
    }
  }

  const items: EssentialItem[] = [
    { label: 'Status', value: <StatusPill phase={detail.data.phase} /> },
    { label: 'Project', value: current },
    { label: 'Size', value: data ? `${data.size_gib} GiB` : '—' },
    {
      label: 'Attached to',
      value: attachedTo ? (
        <Link to={`/compute/virtual-machines/${encodeURIComponent(attachedTo)}`}>{attachedTo}</Link>
      ) : (
        'Unattached'
      ),
    },
    { label: 'Volume ID', value: data?.volid ?? 'Not yet allocated' },
    { label: 'Created', value: new Date(detail.data.created_at).toLocaleString() },
  ]

  return (
    <div className="az-stack-col az-gap-4">
      <h2>Overview</h2>
      <CommandBar>
        {isAttached ? (
          <Button variant="default" onClick={() => setConfirmDetachOpen(true)}>
            Detach
          </Button>
        ) : (
          <Button
            variant="default"
            onClick={() => setAttachOpen(true)}
            disabled={vms.length === 0}
          >
            Attach
          </Button>
        )}
        <Button
          variant="default"
          onClick={() => {
            setResizeValue(data?.size_gib ?? 0)
            setResizeOpen(true)
          }}
        >
          Resize
        </Button>
        <Button
          variant="default"
          onClick={() => setConfirmDeleteOpen(true)}
          disabled={isAttached}
          title={isAttached ? 'Detach this disk before deleting it' : undefined}
        >
          Delete
        </Button>
      </CommandBar>
      <EssentialsGrid items={items} />

      <Modal open={attachOpen} title="Attach disk" onClose={() => setAttachOpen(false)}>
        <div className="az-stack-col az-gap-4">
          <Select
            label="VM"
            value={attachVm}
            onChange={(e) => setAttachVm(e.target.value)}
            hint="Must be on the same node this disk was created on."
          >
            <option value="" disabled>
              Select a VM
            </option>
            {vms.map((vm) => (
              <option key={vm.id} value={vm.name}>
                {vm.name}
              </option>
            ))}
          </Select>
          {actionError && <p className="az-alert az-alert-danger">{actionError}</p>}
          <div className="az-stack-row az-gap-2">
            <Button
              variant="primary"
              onClick={() => runUpdate({ vm_name: attachVm })}
              disabled={updateDisk.isPending || !attachVm}
            >
              Attach
            </Button>
            <Button variant="default" onClick={() => setAttachOpen(false)}>
              Cancel
            </Button>
          </div>
        </div>
      </Modal>

      <Modal open={confirmDetachOpen} title="Detach disk" onClose={() => setConfirmDetachOpen(false)}>
        <div className="az-stack-col az-gap-4">
          <p>
            Detach <strong>{name}</strong> from <strong>{attachedTo}</strong>? The disk's data is
            kept and it can be reattached later.
          </p>
          {actionError && <p className="az-alert az-alert-danger">{actionError}</p>}
          <div className="az-stack-row az-gap-2">
            <Button
              variant="primary"
              onClick={() => runUpdate({ detach: true })}
              disabled={updateDisk.isPending}
            >
              Detach
            </Button>
            <Button variant="default" onClick={() => setConfirmDetachOpen(false)}>
              Cancel
            </Button>
          </div>
        </div>
      </Modal>

      <Modal open={resizeOpen} title="Resize disk" onClose={() => setResizeOpen(false)}>
        <div className="az-stack-col az-gap-4">
          <TextField
            label="Size (GiB)"
            type="number"
            min={data?.size_gib ?? 1}
            value={resizeValue}
            onChange={(e) => setResizeValue(Number(e.target.value))}
            hint="Disks can only grow, not shrink."
          />
          {actionError && <p className="az-alert az-alert-danger">{actionError}</p>}
          <div className="az-stack-row az-gap-2">
            <Button
              variant="primary"
              onClick={() => runUpdate({ size_gib: resizeValue })}
              disabled={updateDisk.isPending || !data || resizeValue <= data.size_gib}
            >
              Resize
            </Button>
            <Button variant="default" onClick={() => setResizeOpen(false)}>
              Cancel
            </Button>
          </div>
        </div>
      </Modal>

      <Modal open={confirmDeleteOpen} title="Delete disk" onClose={() => setConfirmDeleteOpen(false)}>
        <div className="az-stack-col az-gap-4">
          <p>
            Delete disk <strong>{name}</strong>? This cannot be undone.
          </p>
          <div className="az-stack-row az-gap-2">
            <Button variant="primary" onClick={handleDelete} disabled={deleteResource.isPending}>
              Delete
            </Button>
            <Button variant="default" onClick={() => setConfirmDeleteOpen(false)}>
              Cancel
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  )
}

import { useState } from 'react'
import { useNavigate, useOutletContext } from 'react-router-dom'
import { useDeleteProvider, type ProviderDetail } from '../../../api/providers'
import { useCurrentProject } from '../../../hooks/useCurrentProject'
import { useResources } from '../../../api/resources'
import { Button } from '../../../ui/Button'
import { CommandBar } from '../../../ui/CommandBar'
import { EssentialsGrid, type EssentialItem } from '../../../ui/EssentialsGrid'
import { Modal } from '../../../ui/Modal'

export function OverviewTab() {
  const host = useOutletContext<ProviderDetail>()
  const navigate = useNavigate()
  const deleteProvider = useDeleteProvider()
  const { current } = useCurrentProject()
  const resources = useResources(current ?? '')

  const [confirmOpen, setConfirmOpen] = useState(false)

  const handleDelete = async () => {
    await deleteProvider.mutateAsync(host.id)
    navigate('/infrastructure/proxmox-hosts')
  }

  const vmCount = (resources.data ?? []).filter(
    (r) => r.resource_type === 'vm' && r.provider_id === host.id,
  ).length

  const items: EssentialItem[] = [
    { label: 'Type', value: 'Proxmox' },
    { label: 'URL', value: host.config.url },
    { label: 'Node', value: host.config.node },
    { label: 'Default storage', value: host.config.default_storage },
    { label: 'Default bridge', value: host.config.default_bridge },
    {
      label: 'Virtual machines',
      value: current ? vmCount : `— (select a project)`,
    },
    { label: 'Created', value: new Date(host.created_at).toLocaleString() },
  ]

  return (
    <div className="az-stack-col az-gap-4">
      <h2>Overview</h2>
      <CommandBar>
        <Button variant="default" onClick={() => setConfirmOpen(true)}>
          Delete
        </Button>
      </CommandBar>
      <EssentialsGrid items={items} />

      <Modal open={confirmOpen} title="Delete Proxmox host" onClose={() => setConfirmOpen(false)}>
        <div className="az-stack-col az-gap-4">
          <p>
            Delete Proxmox host <strong>{host.name}</strong>? This cannot be undone.
          </p>
          <div className="az-stack-row az-gap-2">
            <Button variant="primary" onClick={handleDelete} disabled={deleteProvider.isPending}>
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

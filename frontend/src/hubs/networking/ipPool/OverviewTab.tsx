import { useState } from 'react'
import { useNavigate, useOutletContext } from 'react-router-dom'
import { type IpPoolDetail, useDeleteIpPool } from '../../../api/ipPools'
import { Button } from '../../../ui/Button'
import { CommandBar } from '../../../ui/CommandBar'
import { EssentialsGrid, type EssentialItem } from '../../../ui/EssentialsGrid'
import { Modal } from '../../../ui/Modal'

export function OverviewTab() {
  const pool = useOutletContext<IpPoolDetail>()
  const navigate = useNavigate()
  const deleteIpPool = useDeleteIpPool()

  const [confirmOpen, setConfirmOpen] = useState(false)

  const handleDelete = async () => {
    await deleteIpPool.mutateAsync(pool.name)
    navigate('/networking/ip-pools')
  }

  const hasAllocations = (pool.allocated ?? 0) > 0

  const items: EssentialItem[] = [
    { label: 'CIDR', value: pool.cidr },
    { label: 'Range', value: `${pool.range_start} – ${pool.range_end}` },
    { label: 'Gateway', value: pool.gateway },
    { label: 'DNS servers', value: pool.dns.length > 0 ? pool.dns.join(', ') : '—' },
    { label: 'Bridge', value: pool.bridge },
    {
      label: 'Allocated / available',
      value:
        pool.allocated === null || pool.available === null
          ? 'Pending…'
          : `${pool.allocated} / ${pool.available}`,
    },
  ]

  return (
    <div className="az-stack-col az-gap-4">
      <h2>Overview</h2>
      <CommandBar>
        <Button
          variant="default"
          onClick={() => setConfirmOpen(true)}
          disabled={hasAllocations}
          title={
            hasAllocations
              ? `${pool.allocated} address(es) still allocated — release them before deleting`
              : undefined
          }
        >
          Delete
        </Button>
      </CommandBar>
      <EssentialsGrid items={items} />

      <Modal open={confirmOpen} title="Delete IP pool" onClose={() => setConfirmOpen(false)}>
        <div className="az-stack-col az-gap-4">
          <p>
            Delete IP pool <strong>{pool.name}</strong>? This cannot be undone.
          </p>
          <div className="az-stack-row az-gap-2">
            <Button variant="primary" onClick={handleDelete} disabled={deleteIpPool.isPending}>
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

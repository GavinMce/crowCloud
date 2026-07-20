import { useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useCurrentProject } from '../../hooks/useCurrentProject'
import { useDeleteResource, useDownloadKubeconfig, useResource } from '../../api/resources'
import { ApiError } from '../../api/client'
import { Button } from '../../ui/Button'
import { CommandBar } from '../../ui/CommandBar'
import { EssentialsGrid, type EssentialItem } from '../../ui/EssentialsGrid'
import { Modal } from '../../ui/Modal'
import { StatusPill } from '../../ui/StatusPill'

interface ParsedK8sHandle {
  controlPlaneIp: string | null
  workerCount: number
  hasKubeconfig: boolean
}

function parseK8sHandle(handle: unknown): ParsedK8sHandle | null {
  if (!handle || typeof handle !== 'object') return null
  // `resources.handle` is always the outer `ResourceHandle{resource_type,
  // data}` envelope, never the driver's handle shape directly — matches
  // every backend reader of this same column (e.g. the bootstrap-callback
  // route, the kubeconfig-download route).
  const data = (handle as { data?: unknown }).data
  if (!data || typeof data !== 'object') return null
  const h = data as {
    control_plane?: { ip?: unknown }
    workers?: unknown[]
    kubeconfig?: unknown
  }
  const ip = h.control_plane?.ip
  return {
    controlPlaneIp: typeof ip === 'string' ? ip : null,
    workerCount: Array.isArray(h.workers) ? h.workers.length : 0,
    hasKubeconfig: typeof h.kubeconfig === 'string',
  }
}

function downloadKubeconfig(name: string, contents: string) {
  const blob = new Blob([contents], { type: 'application/yaml' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `${name}.kubeconfig.yaml`
  a.click()
  URL.revokeObjectURL(url)
}

export function KubernetesClusterOverviewTab() {
  const { name = '' } = useParams()
  const navigate = useNavigate()
  const { current } = useCurrentProject()
  const detail = useResource(current ?? '', name)
  const deleteResource = useDeleteResource(current ?? '')
  const downloadHook = useDownloadKubeconfig(current ?? '')

  const [confirmOpen, setConfirmOpen] = useState(false)
  const [kubeconfigError, setKubeconfigError] = useState<string | null>(null)

  const handleDelete = async () => {
    await deleteResource.mutateAsync(name)
    navigate('/containers/kubernetes-clusters')
  }

  const handleDownload = async () => {
    setKubeconfigError(null)
    try {
      const { kubeconfig } = await downloadHook.mutateAsync(name)
      downloadKubeconfig(name, kubeconfig)
    } catch (err) {
      setKubeconfigError(
        err instanceof ApiError ? err.message : 'Failed to download kubeconfig',
      )
    }
  }

  if (detail.isLoading) {
    return <p>Loading…</p>
  }

  if (detail.isError || !detail.data) {
    return <p className="az-alert az-alert-danger">Failed to load this Kubernetes cluster.</p>
  }

  const parsed = parseK8sHandle(detail.data.handle)
  const nodeCount = parsed ? 1 + parsed.workerCount : null

  const items: EssentialItem[] = [
    { label: 'Status', value: <StatusPill phase={detail.data.phase} /> },
    { label: 'Project', value: current },
    {
      label: 'API endpoint',
      value: parsed?.controlPlaneIp ? `https://${parsed.controlPlaneIp}:6443` : 'Not assigned yet',
    },
    { label: 'Nodes', value: nodeCount !== null ? `${nodeCount} (1 control plane)` : '—' },
    { label: 'Created', value: new Date(detail.data.created_at).toLocaleString() },
  ]

  return (
    <div className="az-stack-col az-gap-4">
      <h2>Overview</h2>
      <CommandBar>
        <Button
          variant="primary"
          onClick={handleDownload}
          disabled={downloadHook.isPending || !parsed?.hasKubeconfig}
        >
          Download kubeconfig
        </Button>
        <Button variant="default" onClick={() => setConfirmOpen(true)}>
          Delete
        </Button>
      </CommandBar>
      {!parsed?.hasKubeconfig && (
        <p className="az-text-secondary">
          Kubeconfig isn't available until the control plane finishes bootstrapping.
        </p>
      )}
      {kubeconfigError && <p className="az-alert az-alert-danger">{kubeconfigError}</p>}
      <EssentialsGrid items={items} />

      <Modal
        open={confirmOpen}
        title="Delete Kubernetes cluster"
        onClose={() => setConfirmOpen(false)}
      >
        <div className="az-stack-col az-gap-4">
          <p>
            Delete Kubernetes cluster <strong>{name}</strong>? This deletes the control plane and
            all worker VMs. This cannot be undone.
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

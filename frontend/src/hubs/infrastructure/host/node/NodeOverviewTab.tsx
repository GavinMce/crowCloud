import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { useOutletContext } from 'react-router-dom'
import { useConfigureProviderNode, useProviderNode } from '../../../../api/providerNodes'
import { useCurrentProject } from '../../../../hooks/useCurrentProject'
import { useResources } from '../../../../api/resources'
import { ApiError } from '../../../../api/client'
import { Button } from '../../../../ui/Button'
import { CommandBar } from '../../../../ui/CommandBar'
import { EssentialsGrid, type EssentialItem } from '../../../../ui/EssentialsGrid'
import { TextField } from '../../../../ui/TextField'
import type { NodeOutletContext } from './NodeLayout'
import { formatCpu, formatMemory, formatUptime, nodeStatusVariant } from '../formatNodeStats'

export function NodeOverviewTab() {
  const { hostId, hostName, nodeName } = useOutletContext<NodeOutletContext>()
  const node = useProviderNode(hostId, nodeName)
  const configureNode = useConfigureProviderNode(hostId)
  const { current } = useCurrentProject()
  const resources = useResources(current ?? '')

  const [editing, setEditing] = useState(false)
  const [form, setForm] = useState({ defaultStorage: '', defaultBridge: '' })
  const [error, setError] = useState<string | null>(null)

  // Keep the form in sync with the loaded/refetched config, but don't
  // clobber in-progress edits.
  useEffect(() => {
    if (node.data && !editing) {
      setForm({
        defaultStorage: node.data.default_storage ?? '',
        defaultBridge: node.data.default_bridge ?? '',
      })
    }
  }, [node.data, editing])

  if (node.isLoading) {
    return <p>Loading…</p>
  }

  if (node.isError || !node.data) {
    return <p className="az-alert az-alert-danger">Failed to load this node.</p>
  }

  const handleSave = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    try {
      await configureNode.mutateAsync({
        name: nodeName,
        default_storage: form.defaultStorage,
        default_bridge: form.defaultBridge,
      })
      setEditing(false)
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to configure node')
    }
  }

  // Every VM on this host lands on its one configured node until node-aware
  // placement exists (issue #32), so "VMs on this node" and "VMs on this
  // host" are the same count for now.
  const vmCount = (resources.data ?? []).filter(
    (r) => r.resource_type === 'vm' && r.provider_id === hostId,
  ).length

  const infoItems: EssentialItem[] = [
    { label: 'Host', value: hostName },
    { label: 'Virtual machines', value: current ? vmCount : '— (select a project)' },
  ]

  const configItems: EssentialItem[] = [
    { label: 'Default storage', value: node.data.default_storage },
    { label: 'Default bridge', value: node.data.default_bridge },
  ]

  const liveItems: EssentialItem[] = [
    {
      label: 'Status',
      value: (
        <span className={`az-pill az-pill-${nodeStatusVariant(node.data.status)}`}>
          {node.data.status}
        </span>
      ),
    },
    { label: 'CPU', value: formatCpu(node.data.cpu, node.data.max_cpu) },
    { label: 'Memory', value: formatMemory(node.data.mem, node.data.max_mem) },
    { label: 'Uptime', value: formatUptime(node.data.uptime) },
  ]

  return (
    <div className="az-stack-col az-gap-4">
      <h2>Overview</h2>
      <EssentialsGrid items={infoItems} />

      <div>
        <h2>Live status</h2>
        <EssentialsGrid items={liveItems} />
      </div>

      <div>
        <div className="az-stack-row az-justify-between az-gap-2">
          <h2>Configuration</h2>
          {!editing && node.data.configured && (
            <CommandBar>
              <Button variant="default" size="sm" onClick={() => setEditing(true)}>
                Edit
              </Button>
            </CommandBar>
          )}
        </div>

        {!editing && node.data.configured && <EssentialsGrid items={configItems} />}

        {!editing && !node.data.configured && (
          <div className="az-placeholder">
            <p style={{ margin: 0, fontWeight: 600 }}>Not configured</p>
            <p style={{ margin: '8px 0 16px' }}>
              This node was discovered but has no default storage/bridge set — VMs can't be
              provisioned here until it's adopted.
            </p>
            <Button variant="primary" onClick={() => setEditing(true)}>
              Adopt this node
            </Button>
          </div>
        )}

        {editing && (
          <form onSubmit={handleSave}>
            <div className="az-stack-col az-gap-4" style={{ maxWidth: 420 }}>
              <TextField
                label="Default storage"
                value={form.defaultStorage}
                onChange={(e) => setForm({ ...form, defaultStorage: e.target.value })}
                required
                autoFocus
              />
              <TextField
                label="Default bridge"
                value={form.defaultBridge}
                onChange={(e) => setForm({ ...form, defaultBridge: e.target.value })}
                required
              />
              {error && <p className="az-alert az-alert-danger">{error}</p>}
              <div className="az-stack-row az-gap-2">
                <Button type="submit" variant="primary" disabled={configureNode.isPending}>
                  Save
                </Button>
                <Button
                  type="button"
                  variant="default"
                  onClick={() => {
                    setEditing(false)
                    setError(null)
                  }}
                >
                  Cancel
                </Button>
              </div>
            </div>
          </form>
        )}
      </div>
    </div>
  )
}

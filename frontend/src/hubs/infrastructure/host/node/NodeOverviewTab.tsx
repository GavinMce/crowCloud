import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { useNavigate, useOutletContext } from 'react-router-dom'
import { useProvider, useUpdateProvider } from '../../../../api/providers'
import { useCurrentProject } from '../../../../hooks/useCurrentProject'
import { useResources } from '../../../../api/resources'
import { ApiError } from '../../../../api/client'
import { Button } from '../../../../ui/Button'
import { CommandBar } from '../../../../ui/CommandBar'
import { EssentialsGrid, type EssentialItem } from '../../../../ui/EssentialsGrid'
import { TextField } from '../../../../ui/TextField'
import type { NodeOutletContext } from './NodeLayout'

export function NodeOverviewTab() {
  const { hostId, hostName, nodeName } = useOutletContext<NodeOutletContext>()
  const navigate = useNavigate()
  const host = useProvider(hostId)
  const updateProvider = useUpdateProvider()
  const { current } = useCurrentProject()
  const resources = useResources(current ?? '')

  const [editing, setEditing] = useState(false)
  const [form, setForm] = useState({ node: '', defaultStorage: '', defaultBridge: '' })
  const [error, setError] = useState<string | null>(null)

  // Keep the form in sync with the loaded/refetched config, but don't
  // clobber in-progress edits.
  useEffect(() => {
    if (host.data && !editing) {
      setForm({
        node: host.data.config.node,
        defaultStorage: host.data.config.default_storage,
        defaultBridge: host.data.config.default_bridge,
      })
    }
  }, [host.data, editing])

  if (host.isLoading) {
    return <p>Loading…</p>
  }

  if (host.isError || !host.data) {
    return <p className="az-alert az-alert-danger">Failed to load this node.</p>
  }

  const handleSave = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    try {
      await updateProvider.mutateAsync({
        id: hostId,
        config: {
          node: form.node,
          default_storage: form.defaultStorage,
          default_bridge: form.defaultBridge,
        },
      })
      setEditing(false)
      if (form.node !== nodeName) {
        navigate(
          `/infrastructure/proxmox-hosts/${hostId}/nodes/${encodeURIComponent(form.node)}/overview`,
          { replace: true },
        )
      }
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to update node')
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
    { label: 'Node name', value: host.data.config.node },
    { label: 'Default storage', value: host.data.config.default_storage },
    { label: 'Default bridge', value: host.data.config.default_bridge },
  ]

  const unavailable: EssentialItem[] = [
    { label: 'Status', value: 'Not available yet' },
    { label: 'Proxmox version', value: 'Not available yet' },
    { label: 'CPU', value: 'Not available yet' },
    { label: 'Memory', value: 'Not available yet' },
    { label: 'Local storage usage', value: 'Not available yet' },
  ]

  return (
    <div className="az-stack-col az-gap-4">
      <h2>Overview</h2>
      <EssentialsGrid items={infoItems} />

      <div>
        <div className="az-stack-row az-justify-between az-gap-2">
          <h2>Configuration</h2>
          {!editing && (
            <CommandBar>
              <Button variant="default" size="sm" onClick={() => setEditing(true)}>
                Edit
              </Button>
            </CommandBar>
          )}
        </div>

        {!editing && <EssentialsGrid items={configItems} />}

        {editing && (
          <form onSubmit={handleSave}>
            <div className="az-stack-col az-gap-4" style={{ maxWidth: 420 }}>
              <TextField
                label="Node name"
                value={form.node}
                onChange={(e) => setForm({ ...form, node: e.target.value })}
                required
                autoFocus
              />
              <TextField
                label="Default storage"
                value={form.defaultStorage}
                onChange={(e) => setForm({ ...form, defaultStorage: e.target.value })}
                required
              />
              <TextField
                label="Default bridge"
                value={form.defaultBridge}
                onChange={(e) => setForm({ ...form, defaultBridge: e.target.value })}
                required
              />
              {error && <p className="az-alert az-alert-danger">{error}</p>}
              <div className="az-stack-row az-gap-2">
                <Button type="submit" variant="primary" disabled={updateProvider.isPending}>
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

      <div>
        <h2>Live status</h2>
        <p className="az-text-secondary">
          Requires querying this node directly, which isn&apos;t wired up yet — see{' '}
          <a href="https://github.com/GavinMce/crowCloud/issues/32" target="_blank" rel="noreferrer">
            issue #32
          </a>
          .
        </p>
        <EssentialsGrid items={unavailable} />
      </div>
    </div>
  )
}

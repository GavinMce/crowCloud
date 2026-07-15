import { useState } from 'react'
import type { FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { useCreateProvider } from '../../api/providers'
import { ApiError } from '../../api/client'
import { Breadcrumb } from '../../ui/Breadcrumb'
import { Button } from '../../ui/Button'
import { Tabs } from '../../ui/Tabs'
import { TextField } from '../../ui/TextField'

const TABS = [
  { id: 'basics', label: 'Basics' },
  { id: 'node', label: 'Initial node' },
]

export function CreateProxmoxHostPage() {
  const navigate = useNavigate()
  const createProvider = useCreateProvider()

  const [tab, setTab] = useState('basics')
  const [error, setError] = useState<string | null>(null)
  const [form, setForm] = useState({
    name: '',
    url: '',
    tokenId: '',
    tokenSecret: '',
    node: '',
    defaultStorage: '',
    defaultBridge: '',
    tlsInsecure: false,
  })

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    try {
      const host = await createProvider.mutateAsync({
        name: form.name,
        provider_type: 'proxmox',
        config: {
          url: form.url,
          token_id: form.tokenId,
          token_secret: form.tokenSecret,
          node: form.node,
          default_storage: form.defaultStorage,
          default_bridge: form.defaultBridge,
          tls_insecure: form.tlsInsecure,
        },
      })
      navigate(`/infrastructure/proxmox-hosts/${host.id}`)
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to add Proxmox host')
    }
  }

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <Breadcrumb
          items={[{ label: 'Proxmox hosts', to: '/infrastructure/proxmox-hosts' }, { label: 'Create' }]}
        />
        <h1>Add a Proxmox host</h1>
        <Tabs tabs={TABS} activeTab={tab} onChange={setTab} />

        <form onSubmit={handleSubmit}>
          {tab === 'basics' && (
            <div className="az-stack-col az-gap-4" style={{ maxWidth: 480 }}>
              <TextField
                label="Name"
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                required
                autoFocus
              />
              <TextField
                label="URL"
                placeholder="https://pve.example.com:8006"
                value={form.url}
                onChange={(e) => setForm({ ...form, url: e.target.value })}
                required
              />
              <TextField
                label="Token ID"
                placeholder="root@pam!crow"
                value={form.tokenId}
                onChange={(e) => setForm({ ...form, tokenId: e.target.value })}
                required
              />
              <TextField
                label="Token Secret"
                type="password"
                value={form.tokenSecret}
                onChange={(e) => setForm({ ...form, tokenSecret: e.target.value })}
                required
              />
              <label className="az-stack-row az-gap-2">
                <input
                  type="checkbox"
                  checked={form.tlsInsecure}
                  onChange={(e) => setForm({ ...form, tlsInsecure: e.target.checked })}
                />
                Allow insecure TLS
              </label>
              <div>
                <Button type="button" variant="primary" onClick={() => setTab('node')}>
                  Next: Initial node
                </Button>
              </div>
            </div>
          )}

          {tab === 'node' && (
            <div className="az-stack-col az-gap-4" style={{ maxWidth: 480 }}>
              <p className="az-text-secondary">
                crowCloud can&apos;t discover a host&apos;s nodes yet — for now, tell us about the
                node to provision VMs on. This moves to its own Nodes tab, auto-discovered, once{' '}
                <a href="https://github.com/GavinMce/crowCloud/issues/32" target="_blank" rel="noreferrer">
                  issue #32
                </a>{' '}
                lands.
              </p>
              <TextField
                label="Node name"
                value={form.node}
                onChange={(e) => setForm({ ...form, node: e.target.value })}
                required
                hint="Proxmox node name, e.g. pve"
              />
              <TextField
                label="Default storage"
                value={form.defaultStorage}
                onChange={(e) => setForm({ ...form, defaultStorage: e.target.value })}
                required
                hint="Storage ID for clones and cloud-init snippets, e.g. local-lvm"
              />
              <TextField
                label="Default bridge"
                value={form.defaultBridge}
                onChange={(e) => setForm({ ...form, defaultBridge: e.target.value })}
                required
                hint="Linux bridge for new VM NICs, e.g. vmbr0"
              />
              {error && <p className="az-alert az-alert-danger">{error}</p>}
              <div className="az-stack-row az-gap-2">
                <Button
                  type="submit"
                  variant="primary"
                  disabled={
                    createProvider.isPending ||
                    !form.name ||
                    !form.url ||
                    !form.tokenId ||
                    !form.tokenSecret ||
                    !form.node ||
                    !form.defaultStorage ||
                    !form.defaultBridge
                  }
                >
                  Create
                </Button>
                <Button type="button" variant="default" onClick={() => setTab('basics')}>
                  Back
                </Button>
              </div>
            </div>
          )}
        </form>
      </div>
    </div>
  )
}

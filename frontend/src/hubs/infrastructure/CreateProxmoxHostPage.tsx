import { useState } from 'react'
import type { FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { useCreateProvider } from '../../api/providers'
import { ApiError } from '../../api/client'
import { Breadcrumb } from '../../ui/Breadcrumb'
import { Button } from '../../ui/Button'
import { TextField } from '../../ui/TextField'

export function CreateProxmoxHostPage() {
  const navigate = useNavigate()
  const createProvider = useCreateProvider()

  const [error, setError] = useState<string | null>(null)
  const [form, setForm] = useState({
    name: '',
    url: '',
    tokenId: '',
    tokenSecret: '',
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
        <p className="az-text-secondary">
          Adds the connection only. Adopt a node (and pick default storage/bridge for it) from the
          host&apos;s Nodes tab once it&apos;s created — nodes are chosen per virtual machine at
          create time, not fixed on the host.
        </p>

        <form onSubmit={handleSubmit}>
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
            {error && <p className="az-alert az-alert-danger">{error}</p>}
            <div>
              <Button
                type="submit"
                variant="primary"
                disabled={
                  createProvider.isPending ||
                  !form.name ||
                  !form.url ||
                  !form.tokenId ||
                  !form.tokenSecret
                }
              >
                Create
              </Button>
            </div>
          </div>
        </form>
      </div>
    </div>
  )
}

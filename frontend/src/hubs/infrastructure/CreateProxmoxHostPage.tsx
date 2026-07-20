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
    sshUser: 'root',
    sshPort: 22,
    sshPrivateKey: '',
    debugSshPublicKey: '',
    kvm: true,
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
          kvm: form.kvm,
          ...(form.sshPrivateKey
            ? { ssh_user: form.sshUser, ssh_port: form.sshPort, ssh_private_key: form.sshPrivateKey }
            : {}),
          ...(form.debugSshPublicKey ? { ssh_public_key: form.debugSshPublicKey } : {}),
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
            <label className="az-stack-row az-gap-2">
              <input
                type="checkbox"
                checked={form.kvm}
                onChange={(e) => setForm({ ...form, kvm: e.target.checked })}
              />
              Hardware-accelerated virtualization (KVM)
            </label>
            {!form.kvm && (
              <p className="az-text-secondary">
                VMs will boot via software emulation instead — only turn this off if the host
                itself has no VT-x/AMD-V available (e.g. it's a nested/virtualized Proxmox
                install). Much slower.
              </p>
            )}

            <h3>SSH (optional)</h3>
            <p className="az-text-secondary">
              Only needed for Kubernetes clusters and any VM with a custom cloud-init script —
              Proxmox's API has no way to upload those, so crowCloud SSHes in directly. Add the
              public key below to <code>root</code>&apos;s <code>authorized_keys</code> on the
              host first. Plain VM creation works fine without this.
            </p>
            <TextField
              label="SSH user"
              value={form.sshUser}
              onChange={(e) => setForm({ ...form, sshUser: e.target.value })}
            />
            <TextField
              label="SSH port"
              type="number"
              min={1}
              value={form.sshPort}
              onChange={(e) => setForm({ ...form, sshPort: Number(e.target.value) })}
            />
            <div className="az-field">
              <label className="az-field-label" htmlFor="ssh-private-key">
                SSH private key
              </label>
              <textarea
                id="ssh-private-key"
                className="az-field-input"
                rows={6}
                placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
                value={form.sshPrivateKey}
                onChange={(e) => setForm({ ...form, sshPrivateKey: e.target.value })}
              />
              <span className="az-field-hint">PEM-encoded, unencrypted (no passphrase)</span>
            </div>

            <TextField
              label="VM debug SSH public key (optional)"
              value={form.debugSshPublicKey}
              onChange={(e) => setForm({ ...form, debugSshPublicKey: e.target.value })}
              placeholder="ssh-ed25519 AAAA..."
              hint="Authorized on every VM's ubuntu user via cloud-init, so a bootstrap script failure is still debuggable afterward. Doesn't need to match the SSH key above."
            />

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

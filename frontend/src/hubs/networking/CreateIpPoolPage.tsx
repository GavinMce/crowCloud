import { useState } from 'react'
import type { FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { useCreateIpPool } from '../../api/ipPools'
import { ApiError } from '../../api/client'
import { Breadcrumb } from '../../ui/Breadcrumb'
import { Button } from '../../ui/Button'
import { Tabs } from '../../ui/Tabs'
import { TextField } from '../../ui/TextField'

const TABS = [
  { id: 'basics', label: 'Basics' },
  { id: 'review', label: 'Review + create' },
]

export function CreateIpPoolPage() {
  const navigate = useNavigate()
  const createIpPool = useCreateIpPool()

  const [tab, setTab] = useState('basics')
  const [error, setError] = useState<string | null>(null)
  const [form, setForm] = useState({
    name: '',
    cidr: '',
    rangeStart: '',
    rangeEnd: '',
    gateway: '',
    dns: '',
    bridge: '',
  })

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    try {
      await createIpPool.mutateAsync({
        name: form.name,
        cidr: form.cidr,
        range_start: form.rangeStart,
        range_end: form.rangeEnd,
        gateway: form.gateway,
        dns: form.dns
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
        bridge: form.bridge,
      })
      navigate('/networking/ip-pools')
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to create IP pool')
    }
  }

  const requiredFilled =
    form.name && form.cidr && form.rangeStart && form.rangeEnd && form.gateway && form.bridge

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <Breadcrumb items={[{ label: 'IP pools', to: '/networking/ip-pools' }, { label: 'Create' }]} />
        <h1>Create an IP pool</h1>
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
                hint="Lowercase letters, numbers, and hyphens only"
              />
              <TextField
                label="CIDR"
                value={form.cidr}
                onChange={(e) => setForm({ ...form, cidr: e.target.value })}
                required
                hint="e.g. 10.20.0.0/24"
              />
              <TextField
                label="Range start"
                value={form.rangeStart}
                onChange={(e) => setForm({ ...form, rangeStart: e.target.value })}
                required
                hint="First allocatable address within the CIDR"
              />
              <TextField
                label="Range end"
                value={form.rangeEnd}
                onChange={(e) => setForm({ ...form, rangeEnd: e.target.value })}
                required
                hint="Last allocatable address within the CIDR"
              />
              <TextField
                label="Gateway"
                value={form.gateway}
                onChange={(e) => setForm({ ...form, gateway: e.target.value })}
                required
              />
              <TextField
                label="DNS servers"
                value={form.dns}
                onChange={(e) => setForm({ ...form, dns: e.target.value })}
                hint="Comma-separated, e.g. 1.1.1.1, 8.8.8.8"
              />
              <TextField
                label="Bridge"
                value={form.bridge}
                onChange={(e) => setForm({ ...form, bridge: e.target.value })}
                required
                hint="e.g. vmbr0"
              />
              <div>
                <Button type="button" variant="primary" onClick={() => setTab('review')}>
                  Next: Review + create
                </Button>
              </div>
            </div>
          )}

          {tab === 'review' && (
            <div className="az-stack-col az-gap-4" style={{ maxWidth: 480 }}>
              <div className="az-card">
                <dl className="az-stack-col az-gap-2">
                  <div>
                    <strong>Name:</strong> {form.name || '—'}
                  </div>
                  <div>
                    <strong>CIDR:</strong> {form.cidr || '—'}
                  </div>
                  <div>
                    <strong>Range:</strong>{' '}
                    {form.rangeStart && form.rangeEnd
                      ? `${form.rangeStart} – ${form.rangeEnd}`
                      : '—'}
                  </div>
                  <div>
                    <strong>Gateway:</strong> {form.gateway || '—'}
                  </div>
                  <div>
                    <strong>DNS servers:</strong> {form.dns || '—'}
                  </div>
                  <div>
                    <strong>Bridge:</strong> {form.bridge || '—'}
                  </div>
                </dl>
              </div>
              {error && <p className="az-alert az-alert-danger">{error}</p>}
              <div className="az-stack-row az-gap-2">
                <Button
                  type="submit"
                  variant="primary"
                  disabled={createIpPool.isPending || !requiredFilled}
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

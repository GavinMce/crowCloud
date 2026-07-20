import { useParams } from 'react-router-dom'
import { useCurrentProject } from '../../hooks/useCurrentProject'
import { useClusterMetrics, type NodeMetric } from '../../api/k8sMetrics'
import { ApiError } from '../../api/client'
import { StatusPill } from '../../ui/StatusPill'

function formatCores(millicores: number | null): string {
  if (millicores === null) return '—'
  return `${(millicores / 1000).toFixed(2)} cores`
}

function formatBytes(bytes: number | null): string {
  if (bytes === null) return '—'
  const gib = bytes / 1024 ** 3
  if (gib >= 1) return `${gib.toFixed(2)} GiB`
  return `${(bytes / 1024 ** 2).toFixed(0)} MiB`
}

function meterFillClass(pct: number): string {
  if (pct >= 90) return 'az-meter-fill az-meter-fill-danger'
  if (pct >= 70) return 'az-meter-fill az-meter-fill-warning'
  return 'az-meter-fill'
}

function UsageMeter({ used, capacity, label }: { used: number | null; capacity: number | null; label: string }) {
  if (used === null || capacity === null || capacity === 0) {
    return <span className="az-text-secondary">{label}: —</span>
  }
  const pct = Math.min(100, (used / capacity) * 100)
  return (
    <div className="az-stack-col az-gap-1" style={{ minWidth: 160 }}>
      <div className="az-meter">
        <div className={meterFillClass(pct)} style={{ width: `${pct}%` }} />
      </div>
      <span className="az-text-secondary">
        {label}: {pct.toFixed(0)}%
      </span>
    </div>
  )
}

function NodeRow({ node }: { node: NodeMetric }) {
  return (
    <tr>
      <td>{node.name}</td>
      <td>
        <StatusPill phase={node.ready ? 'Ready' : 'NotReady'} />
      </td>
      <td>
        <UsageMeter
          used={node.cpu_usage_millicores}
          capacity={node.cpu_capacity_millicores}
          label={`${formatCores(node.cpu_usage_millicores)} / ${formatCores(node.cpu_capacity_millicores)}`}
        />
      </td>
      <td>
        <UsageMeter
          used={node.memory_usage_bytes}
          capacity={node.memory_capacity_bytes}
          label={`${formatBytes(node.memory_usage_bytes)} / ${formatBytes(node.memory_capacity_bytes)}`}
        />
      </td>
    </tr>
  )
}

export function KubernetesClusterMetricsTab() {
  const { name = '' } = useParams()
  const { current } = useCurrentProject()
  const metrics = useClusterMetrics(current ?? '', name)

  return (
    <div className="az-stack-col az-gap-4">
      <h2>Monitoring</h2>
      <p className="az-text-secondary">
        Live node CPU/memory usage from the cluster's built-in metrics-server — no separate
        monitoring stack required. Refreshes every 15 seconds.
      </p>

      {metrics.isLoading && <p>Loading…</p>}
      {metrics.isError && (
        <p className="az-alert az-alert-danger">
          {metrics.error instanceof ApiError
            ? metrics.error.message
            : 'Failed to load cluster metrics.'}
        </p>
      )}
      {metrics.data && metrics.data.nodes.length === 0 && <p>No nodes reported yet.</p>}
      {metrics.data && metrics.data.nodes.length > 0 && (
        <table className="az-table">
          <thead>
            <tr>
              <th>Node</th>
              <th>Status</th>
              <th>CPU</th>
              <th>Memory</th>
            </tr>
          </thead>
          <tbody>
            {metrics.data.nodes.map((node) => (
              <NodeRow key={node.name} node={node} />
            ))}
          </tbody>
        </table>
      )}
    </div>
  )
}

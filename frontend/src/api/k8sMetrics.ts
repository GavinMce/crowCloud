import { useQuery } from '@tanstack/react-query'
import { apiFetch } from './client'

export interface NodeMetric {
  name: string
  ready: boolean
  cpu_usage_millicores: number | null
  cpu_capacity_millicores: number | null
  memory_usage_bytes: number | null
  memory_capacity_bytes: number | null
}

export interface ClusterMetricsResponse {
  nodes: NodeMetric[]
}

/** Sourced from K3s's bundled metrics-server — no opt-in monitoring stack
 * required. Polls every 15s while the tab is open; 409s until the cluster
 * finishes bootstrapping (no kubeconfig yet), surfaced as a normal error. */
export function useClusterMetrics(project: string, name: string) {
  return useQuery({
    queryKey: ['k8s-metrics', project, name],
    queryFn: () =>
      apiFetch<ClusterMetricsResponse>(
        `/projects/${encodeURIComponent(project)}/resources/${encodeURIComponent(name)}/metrics`,
      ),
    enabled: project.length > 0 && name.length > 0,
    refetchInterval: 15000,
    retry: false,
  })
}

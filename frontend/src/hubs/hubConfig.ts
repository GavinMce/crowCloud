import type { ComponentType } from 'react'
import {
  ComputeIcon,
  ContainersIcon,
  DatabaseIcon,
  NetworkIcon,
  ServerIcon,
  StorageIcon,
} from '../ui/icons'

export type ResourceTypeStatus = 'live' | 'placeholder'

export interface ResourceTypeConfig {
  id: string
  label: string
  status: ResourceTypeStatus
  /** Matches `ResourceRow.resource_type` from the API — only set for `live` types. */
  apiResourceType?: string
  description: string
}

export interface HubConfig {
  id: string
  label: string
  description: string
  icon: ComponentType<{ size?: number }>
  resourceTypes: ResourceTypeConfig[]
}

export const HUBS: HubConfig[] = [
  {
    id: 'compute',
    label: 'Compute',
    description: 'Virtual machines and related compute infrastructure.',
    icon: ComputeIcon,
    resourceTypes: [
      {
        id: 'virtual-machines',
        label: 'Virtual machines',
        status: 'live',
        apiResourceType: 'vm',
        description: 'Provision and manage virtual machines.',
      },
      {
        id: 'images',
        label: 'Images',
        status: 'placeholder',
        description: 'Custom VM images built from existing machines.',
      },
      {
        id: 'disks',
        label: 'Disks',
        status: 'live',
        apiResourceType: 'disk',
        description: 'Standalone managed disks, independent of a VM.',
      },
    ],
  },
  {
    id: 'containers',
    label: 'Containers',
    description: 'Managed Kubernetes clusters.',
    icon: ContainersIcon,
    resourceTypes: [
      {
        id: 'kubernetes-clusters',
        label: 'Kubernetes clusters',
        status: 'placeholder',
        description: 'Managed k3s/RKE2 clusters.',
      },
    ],
  },
  {
    id: 'storage',
    label: 'Storage',
    description: 'Object storage buckets.',
    icon: StorageIcon,
    resourceTypes: [
      {
        id: 'object-storage',
        label: 'Object storage',
        status: 'placeholder',
        description: 'S3-compatible object storage buckets.',
      },
    ],
  },
  {
    id: 'databases',
    label: 'Databases',
    description: 'Managed database instances.',
    icon: DatabaseIcon,
    resourceTypes: [
      {
        id: 'instances',
        label: 'Databases',
        status: 'placeholder',
        description: 'Managed Postgres, MySQL, and MariaDB instances.',
      },
    ],
  },
  {
    id: 'networking',
    label: 'Networking',
    description: 'IP pools, exposed endpoints, and custom domains.',
    icon: NetworkIcon,
    resourceTypes: [
      {
        id: 'ip-pools',
        label: 'IP pools',
        status: 'placeholder',
        description: 'Static IP ranges VMs and clusters can request addresses from.',
      },
      {
        id: 'exposed-endpoints',
        label: 'Exposed endpoints',
        status: 'placeholder',
        description: 'HTTP/TCP expose rules for reaching a resource publicly.',
      },
      {
        id: 'custom-domains',
        label: 'Custom domains',
        status: 'placeholder',
        description: 'Custom domains and TLS certificates.',
      },
    ],
  },
  {
    id: 'infrastructure',
    label: 'Infrastructure',
    description: 'Registered hosts your resources run on.',
    icon: ServerIcon,
    resourceTypes: [
      {
        id: 'proxmox-hosts',
        label: 'Proxmox hosts',
        status: 'live',
        description: 'Proxmox VE hosts available to provision virtual machines on.',
      },
      {
        id: 'router-hosts',
        label: 'Router hosts',
        status: 'placeholder',
        description: 'OPNsense-based router/firewall hosts for networking and exposure.',
      },
    ],
  },
]

export function getHub(id: string): HubConfig | undefined {
  return HUBS.find((h) => h.id === id)
}

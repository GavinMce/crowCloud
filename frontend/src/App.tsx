import { Navigate, Route, Routes } from 'react-router-dom'
import { RequireAuth } from './auth/RequireAuth'
import { AppShell } from './layout/AppShell'
import { HubLayout } from './layout/HubLayout'
import { LoginPage } from './pages/LoginPage'
import { HomePage } from './pages/HomePage'
import { HubOverviewPage } from './hubs/HubOverviewPage'
import { AllResourcesPage } from './hubs/AllResourcesPage'
import { PlaceholderResourceTypePage } from './hubs/PlaceholderResourceTypePage'
import { VirtualMachinesPage } from './hubs/compute/VirtualMachinesPage'
import { CreateVirtualMachinePage } from './hubs/compute/CreateVirtualMachinePage'
import { VirtualMachineLayout } from './hubs/compute/VirtualMachineLayout'
import { VirtualMachineOverviewTab } from './hubs/compute/VirtualMachineOverviewTab'
import { ProjectsPage } from './hubs/management/ProjectsPage'
import { InfrastructureOverviewPage } from './hubs/infrastructure/InfrastructureOverviewPage'
import { AllHostsPage } from './hubs/infrastructure/AllHostsPage'
import { ProxmoxHostsPage } from './hubs/infrastructure/ProxmoxHostsPage'
import { CreateProxmoxHostPage } from './hubs/infrastructure/CreateProxmoxHostPage'
import { ProxmoxHostLayout } from './hubs/infrastructure/host/ProxmoxHostLayout'
import { OverviewTab } from './hubs/infrastructure/host/OverviewTab'
import { NodesTab } from './hubs/infrastructure/host/NodesTab'
import { VirtualMachinesTab } from './hubs/infrastructure/host/VirtualMachinesTab'
import { SettingsTab } from './hubs/infrastructure/host/SettingsTab'
import { NotAvailableTab } from './ui/NotAvailableTab'
import { NodeLayout } from './hubs/infrastructure/host/node/NodeLayout'
import { NodeOverviewTab } from './hubs/infrastructure/host/node/NodeOverviewTab'
import { IpPoolsPage } from './hubs/networking/IpPoolsPage'
import { CreateIpPoolPage } from './hubs/networking/CreateIpPoolPage'
import { IpPoolLayout } from './hubs/networking/ipPool/IpPoolLayout'
import { OverviewTab as IpPoolOverviewTab } from './hubs/networking/ipPool/OverviewTab'

export function App() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route element={<RequireAuth />}>
        <Route element={<AppShell />}>
          <Route path="/" element={<HomePage />} />

          {/* Hub browsing (Overview/All resources/each resource type's
              list) stays nested inside HubLayout, so its nav shows. Create
              and resource-detail routes are top-level siblings below, so
              they replace the hub nav with their own instead of stacking
              on top of it. */}
          <Route path="/compute" element={<HubLayout hubId="compute" />}>
            <Route index element={<Navigate to="overview" replace />} />
            <Route path="overview" element={<HubOverviewPage hubId="compute" />} />
            <Route path="all-resources" element={<AllResourcesPage hubId="compute" />} />
            <Route path="virtual-machines" element={<VirtualMachinesPage />} />
            <Route
              path="images"
              element={<PlaceholderResourceTypePage hubId="compute" typeId="images" />}
            />
            <Route
              path="disks"
              element={<PlaceholderResourceTypePage hubId="compute" typeId="disks" />}
            />
          </Route>
          <Route path="/compute/virtual-machines/create" element={<CreateVirtualMachinePage />} />
          <Route path="/compute/virtual-machines/:name" element={<VirtualMachineLayout />}>
            <Route index element={<Navigate to="overview" replace />} />
            <Route path="overview" element={<VirtualMachineOverviewTab />} />
          </Route>

          <Route path="/containers" element={<HubLayout hubId="containers" />}>
            <Route index element={<Navigate to="overview" replace />} />
            <Route path="overview" element={<HubOverviewPage hubId="containers" />} />
            <Route path="all-resources" element={<AllResourcesPage hubId="containers" />} />
            <Route
              path="kubernetes-clusters"
              element={<PlaceholderResourceTypePage hubId="containers" typeId="kubernetes-clusters" />}
            />
          </Route>

          <Route path="/storage" element={<HubLayout hubId="storage" />}>
            <Route index element={<Navigate to="overview" replace />} />
            <Route path="overview" element={<HubOverviewPage hubId="storage" />} />
            <Route path="all-resources" element={<AllResourcesPage hubId="storage" />} />
            <Route
              path="object-storage"
              element={<PlaceholderResourceTypePage hubId="storage" typeId="object-storage" />}
            />
          </Route>

          <Route path="/databases" element={<HubLayout hubId="databases" />}>
            <Route index element={<Navigate to="overview" replace />} />
            <Route path="overview" element={<HubOverviewPage hubId="databases" />} />
            <Route path="all-resources" element={<AllResourcesPage hubId="databases" />} />
            <Route
              path="instances"
              element={<PlaceholderResourceTypePage hubId="databases" typeId="instances" />}
            />
          </Route>

          <Route path="/networking" element={<HubLayout hubId="networking" />}>
            <Route index element={<Navigate to="overview" replace />} />
            <Route path="overview" element={<HubOverviewPage hubId="networking" />} />
            <Route path="all-resources" element={<AllResourcesPage hubId="networking" />} />
            <Route path="ip-pools" element={<IpPoolsPage />} />
            <Route
              path="exposed-endpoints"
              element={<PlaceholderResourceTypePage hubId="networking" typeId="exposed-endpoints" />}
            />
            <Route
              path="custom-domains"
              element={<PlaceholderResourceTypePage hubId="networking" typeId="custom-domains" />}
            />
          </Route>
          <Route path="/networking/ip-pools/create" element={<CreateIpPoolPage />} />
          <Route path="/networking/ip-pools/:name" element={<IpPoolLayout />}>
            <Route index element={<Navigate to="overview" replace />} />
            <Route path="overview" element={<IpPoolOverviewTab />} />
            <Route
              path="allocations"
              element={
                <NotAvailableTab
                  title="Allocations"
                  description="Which resource holds which address. Depends on a claims-list endpoint (issue #35)."
                />
              }
            />
            <Route
              path="activity-log"
              element={
                <NotAvailableTab
                  title="Activity log"
                  description="crowCloud's audit log doesn't track which resource an action targeted yet."
                />
              }
            />
          </Route>

          <Route path="/infrastructure" element={<HubLayout hubId="infrastructure" />}>
            <Route index element={<Navigate to="overview" replace />} />
            <Route path="overview" element={<InfrastructureOverviewPage />} />
            <Route path="all-resources" element={<AllHostsPage />} />
            <Route path="proxmox-hosts" element={<ProxmoxHostsPage />} />
            <Route
              path="router-hosts"
              element={<PlaceholderResourceTypePage hubId="infrastructure" typeId="router-hosts" />}
            />
          </Route>
          <Route path="/infrastructure/proxmox-hosts/create" element={<CreateProxmoxHostPage />} />
          <Route path="/infrastructure/proxmox-hosts/:id" element={<ProxmoxHostLayout />}>
            <Route index element={<Navigate to="overview" replace />} />
            <Route path="overview" element={<OverviewTab />} />
            <Route path="nodes" element={<NodesTab />} />
            <Route path="virtual-machines" element={<VirtualMachinesTab />} />
            <Route path="settings" element={<SettingsTab />} />
            <Route
              path="storage"
              element={
                <NotAvailableTab
                  title="Storage"
                  description="Storage pools discovered per node. Depends on node discovery (issue #32)."
                />
              }
            />
            <Route
              path="networking"
              element={
                <NotAvailableTab
                  title="Networking"
                  description="Network bridges discovered per node. Depends on node discovery (issue #32)."
                />
              }
            />
            <Route
              path="activity-log"
              element={
                <NotAvailableTab
                  title="Activity log"
                  description="crowCloud's audit log doesn't track which host an action targeted yet."
                />
              }
            />
          </Route>
          <Route
            path="/infrastructure/proxmox-hosts/:id/nodes/:nodeName"
            element={<NodeLayout />}
          >
            <Route index element={<Navigate to="overview" replace />} />
            <Route path="overview" element={<NodeOverviewTab />} />
          </Route>

          <Route path="/management/projects" element={<ProjectsPage />} />
        </Route>
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  )
}

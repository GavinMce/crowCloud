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
import { VirtualMachineDetailPage } from './hubs/compute/VirtualMachineDetailPage'
import { ProjectsPage } from './hubs/management/ProjectsPage'
import { CloudHostsPage } from './hubs/management/CloudHostsPage'

export function App() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route element={<RequireAuth />}>
        <Route element={<AppShell />}>
          <Route path="/" element={<HomePage />} />

          <Route path="/compute" element={<HubLayout hubId="compute" />}>
            <Route index element={<Navigate to="overview" replace />} />
            <Route path="overview" element={<HubOverviewPage hubId="compute" />} />
            <Route path="all-resources" element={<AllResourcesPage hubId="compute" />} />
            <Route path="virtual-machines" element={<VirtualMachinesPage />} />
            <Route path="virtual-machines/create" element={<CreateVirtualMachinePage />} />
            <Route path="virtual-machines/:name" element={<VirtualMachineDetailPage />} />
            <Route
              path="images"
              element={<PlaceholderResourceTypePage hubId="compute" typeId="images" />}
            />
            <Route
              path="disks"
              element={<PlaceholderResourceTypePage hubId="compute" typeId="disks" />}
            />
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
            <Route
              path="ip-pools"
              element={<PlaceholderResourceTypePage hubId="networking" typeId="ip-pools" />}
            />
            <Route
              path="exposed-endpoints"
              element={<PlaceholderResourceTypePage hubId="networking" typeId="exposed-endpoints" />}
            />
            <Route
              path="custom-domains"
              element={<PlaceholderResourceTypePage hubId="networking" typeId="custom-domains" />}
            />
          </Route>

          <Route path="/management/projects" element={<ProjectsPage />} />
          <Route path="/management/cloudhosts" element={<CloudHostsPage />} />
        </Route>
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  )
}

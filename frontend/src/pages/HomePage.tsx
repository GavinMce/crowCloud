import { Card, Container, Grid, GridItem, Stack, Stat } from '@crow-dev/ui'
import { Link } from 'react-router-dom'
import { useProjects } from '../api/projects'
import { useProviders } from '../api/providers'

export function HomePage() {
  const projects = useProjects()
  const providers = useProviders()

  return (
    <Container maxWidth="lg">
      <Stack direction="column" gap={8}>
        <h1>Home</h1>

        <Grid cols={2} gap={4}>
          <GridItem>
            <Stat label="Projects" value={projects.data?.length ?? '—'} />
          </GridItem>
          <GridItem>
            <Stat label="Cloud Hosts" value={providers.data?.length ?? '—'} />
          </GridItem>
        </Grid>

        <section>
          <h2>Get started</h2>
          <Grid cols={2} gap={4}>
            <GridItem>
              <Link to="/projects" style={{ textDecoration: 'none', color: 'inherit' }}>
                <Card header="New Project">
                  <p>Create a project to hold your resources.</p>
                </Card>
              </Link>
            </GridItem>
            <GridItem>
              <Link to="/cloud-hosts" style={{ textDecoration: 'none', color: 'inherit' }}>
                <Card header="Add Cloud Host">
                  <p>Connect a Proxmox host to start provisioning.</p>
                </Card>
              </Link>
            </GridItem>
          </Grid>
        </section>
      </Stack>
    </Container>
  )
}

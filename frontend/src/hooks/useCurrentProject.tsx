import { createContext, useCallback, useContext, useState } from 'react'
import type { ReactNode } from 'react'
import { useProjects } from '../api/projects'

const STORAGE_KEY = 'crow_current_project'

interface ProjectContextValue {
  projectNames: string[]
  isLoading: boolean
  current: string | null
  setCurrent: (name: string | null) => void
}

const ProjectContext = createContext<ProjectContextValue | null>(null)

export function ProjectProvider({ children }: { children: ReactNode }) {
  const projects = useProjects()
  const [current, setCurrentState] = useState<string | null>(() =>
    localStorage.getItem(STORAGE_KEY),
  )

  const setCurrent = useCallback((name: string | null) => {
    setCurrentState(name)
    if (name) {
      localStorage.setItem(STORAGE_KEY, name)
    } else {
      localStorage.removeItem(STORAGE_KEY)
    }
  }, [])

  const value: ProjectContextValue = {
    projectNames: projects.data?.map((p) => p.name) ?? [],
    isLoading: projects.isLoading,
    current,
    setCurrent,
  }

  return <ProjectContext.Provider value={value}>{children}</ProjectContext.Provider>
}

export function useCurrentProject() {
  const ctx = useContext(ProjectContext)
  if (!ctx) throw new Error('useCurrentProject must be used within a ProjectProvider')
  return ctx
}

import type { ReactNode } from 'react'

export function CommandBar({ children }: { children: ReactNode }) {
  return <div className="az-command-bar">{children}</div>
}

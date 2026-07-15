import type { ReactNode } from 'react'

export interface EssentialItem {
  label: string
  value: ReactNode
}

export function EssentialsGrid({ items }: { items: EssentialItem[] }) {
  return (
    <div className="az-essentials">
      {items.map((item) => (
        <div key={item.label}>
          <div className="az-essentials-label">{item.label}</div>
          <div className="az-essentials-value">{item.value}</div>
        </div>
      ))}
    </div>
  )
}

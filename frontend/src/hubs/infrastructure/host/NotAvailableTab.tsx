export function NotAvailableTab({ title, description }: { title: string; description: string }) {
  return (
    <div className="az-stack-col az-gap-4">
      <h2>{title}</h2>
      <div className="az-placeholder">
        <p style={{ margin: 0, fontWeight: 600 }}>Not available yet</p>
        <p style={{ margin: '8px 0 0' }}>{description}</p>
      </div>
    </div>
  )
}

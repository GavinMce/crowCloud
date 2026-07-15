import type { MouseEvent } from 'react'
import { Navbar, type NavbarProps } from '@crow-dev/ui'
import { useNavigate } from 'react-router-dom'

/**
 * @crow-dev/ui's Navbar renders plain `<a href>` tags with no router
 * awareness, which would force a full page reload on every nav click. This
 * wrapper intercepts plain left-clicks on same-origin relative links and
 * routes them through React Router instead, while leaving real `href`s in
 * place so modifier-clicks and "open in new tab" still work natively.
 */
export function RouterNavbar(props: NavbarProps) {
  const navigate = useNavigate()

  const handleClick = (event: MouseEvent<HTMLDivElement>) => {
    if (event.defaultPrevented || event.button !== 0) return
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return

    const anchor = (event.target as HTMLElement).closest('a')
    if (!anchor) return

    const href = anchor.getAttribute('href')
    if (!href || !href.startsWith('/')) return

    event.preventDefault()
    navigate(href)
  }

  return (
    <div onClick={handleClick}>
      <Navbar {...props} />
    </div>
  )
}

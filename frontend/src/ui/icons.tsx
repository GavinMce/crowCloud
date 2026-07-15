import type { ReactNode, SVGProps } from 'react'

export interface IconProps extends SVGProps<SVGSVGElement> {
  size?: number
}

function base(props: IconProps, children: ReactNode) {
  const { size = 20, ...rest } = props
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.4}
      strokeLinecap="round"
      strokeLinejoin="round"
      {...rest}
    >
      {children}
    </svg>
  )
}

export function HomeIcon(props: IconProps) {
  return base(
    props,
    <path d="M3 9.5 10 3l7 6.5M5 8v8h10V8M8 16v-4h4v4" />,
  )
}

export function ComputeIcon(props: IconProps) {
  return base(
    props,
    <>
      <rect x="3" y="4" width="14" height="9" rx="1" />
      <path d="M7 17h6M10 13v4" />
    </>,
  )
}

export function ContainersIcon(props: IconProps) {
  return base(
    props,
    <>
      <path d="M10 2.5 17 6v8l-7 3.5L3 14V6z" />
      <path d="M3 6l7 3.5L17 6M10 9.5V17.5" />
    </>,
  )
}

export function StorageIcon(props: IconProps) {
  return base(
    props,
    <>
      <ellipse cx="10" cy="5" rx="7" ry="2.5" />
      <path d="M3 5v10c0 1.4 3.1 2.5 7 2.5s7-1.1 7-2.5V5" />
      <path d="M3 10c0 1.4 3.1 2.5 7 2.5s7-1.1 7-2.5" />
    </>,
  )
}

export function DatabaseIcon(props: IconProps) {
  return base(
    props,
    <>
      <ellipse cx="10" cy="4.5" rx="6" ry="2.2" />
      <path d="M4 4.5v11c0 1.2 2.7 2.2 6 2.2s6-1 6-2.2v-11" />
      <path d="M4 9.5c0 1.2 2.7 2.2 6 2.2s6-1 6-2.2" />
      <path d="M4 14c0 1.2 2.7 2.2 6 2.2s6-1 6-2.2" />
    </>,
  )
}

export function NetworkIcon(props: IconProps) {
  return base(
    props,
    <>
      <circle cx="10" cy="4" r="2" />
      <circle cx="4" cy="16" r="2" />
      <circle cx="16" cy="16" r="2" />
      <path d="M10 6v4M10 10 4 14M10 10l6 4" />
    </>,
  )
}

export function ServerIcon(props: IconProps) {
  return base(
    props,
    <>
      <rect x="3" y="3" width="14" height="5.5" rx="1" />
      <rect x="3" y="11.5" width="14" height="5.5" rx="1" />
      <path d="M6 5.75h.01M6 14.25h.01" />
    </>,
  )
}

export function ManagementIcon(props: IconProps) {
  return base(
    props,
    <>
      <circle cx="10" cy="10" r="2.5" />
      <path d="M10 3v2M10 15v2M17 10h-2M5 10H3M14.8 5.2l-1.4 1.4M6.6 13.4l-1.4 1.4M14.8 14.8l-1.4-1.4M6.6 6.6 5.2 5.2" />
    </>,
  )
}

export function ChevronRightIcon(props: IconProps) {
  return base(props, <path d="M7.5 4.5 13 10l-5.5 5.5" />)
}

export function ChevronDownIcon(props: IconProps) {
  return base(props, <path d="M4.5 7.5 10 13l5.5-5.5" />)
}

export function SearchIcon(props: IconProps) {
  return base(
    props,
    <>
      <circle cx="8.5" cy="8.5" r="5.5" />
      <path d="m16.5 16.5-3.6-3.6" />
    </>,
  )
}

export function AccountIcon(props: IconProps) {
  return base(
    props,
    <>
      <circle cx="10" cy="7" r="3.2" />
      <path d="M3.5 17c1-3.4 3.9-5.5 6.5-5.5s5.5 2.1 6.5 5.5" />
    </>,
  )
}

export function CloseIcon(props: IconProps) {
  return base(props, <path d="M5 5l10 10M15 5 5 15" />)
}

export function PlusIcon(props: IconProps) {
  return base(props, <path d="M10 4v12M4 10h12" />)
}

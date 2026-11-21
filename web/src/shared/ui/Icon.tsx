const PATHS = {
  overview: 'M4 19h16M6 15h5M6 11h9M6 7h12',
  accounts: 'M4 7.5A2.5 2.5 0 0 1 6.5 5H19v4M4 7.5V17a2 2 0 0 0 2 2h14V9H6.5A2.5 2.5 0 0 1 4 7.5ZM16 14h.01',
  home: 'M4 11.5 12 5l8 6.5M6 10v9h12v-9M10 19v-5h4v5',
  analysis: 'M4 20h16M7 16v-4M12 16V8M17 16v-7M5.5 9.5 10 6l4 3 5-4.5',
  budget: 'M12 3a9 9 0 1 0 9 9h-9V3ZM15 3.5A9 9 0 0 1 20.5 9H15V3.5Z',
  goal: 'M12 21a9 9 0 1 1 9-9M12 16a4 4 0 1 1 4-4M12 12l8-8M17 4h3v3',
  projection: 'M4 19h16M5 15l4-4 3 3 7-8M15 6h4v4',
  fees: 'M6 18 18 6M8.5 9a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3ZM15.5 18a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z',
  link: 'M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1',
  settings:
    'M5 7h9M18 7h1M5 12h2M11 12h8M5 17h8M17 17h2M16 9a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM9 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM15 19a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z',
  plus: 'M12 5v14M5 12h14',
  edit: 'M4 20h4L19 9l-4-4L4 16v4ZM13 7l4 4',
  trash: 'M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13M10 11v5M14 11v5',
  refresh: 'M20 12a8 8 0 0 1-14.3 4.9M4 12a8 8 0 0 1 14.3-4.9M18 3v4.5h-4.5M6 21v-4.5h4.5',
  upload: 'M12 16V4M7 9l5-5 5 5M5 20h14',
  download: 'M12 4v12M7 11l5 5 5-5M5 20h14',
  close: 'M6 6l12 12M18 6 6 18',
  chevron: 'M9 6l6 6-6 6',
  back: 'M15 6l-6 6 6 6',
  more: 'M5 12h.01M12 12h.01M19 12h.01',
  logout: 'M14 5h5v14h-5M10 8l-4 4 4 4M6 12h9',
  bell: 'M6 16v-5a6 6 0 1 1 12 0v5l1.5 2h-15L6 16ZM10 20a2 2 0 0 0 4 0',
  alert: 'M12 9v4M12 16.5h.01M10.3 4.6 3.2 17a2 2 0 0 0 1.7 3h14.2a2 2 0 0 0 1.7-3L13.7 4.6a2 2 0 0 0-3.4 0Z',
  check: 'M5 12.5l4.5 4.5L19 7.5',
  table: 'M4 6h16v12H4zM4 10h16M4 14h16M10 6v12',
  print: 'M7 9V4h10v5M7 17H5a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-2M7 14h10v6H7z',
  eye: 'M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12ZM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z',
  'eye-off':
    'M4 4l16 16M10.6 6.6A9 9 0 0 1 12 5.5c6 0 9.5 6.5 9.5 6.5a17 17 0 0 1-2.6 3.4M6.6 7.6A16 16 0 0 0 2.5 12S6 18.5 12 18.5a9 9 0 0 0 4.4-1.1M9.9 9.9a3 3 0 0 0 4.2 4.2',
} as const

export type IconName = keyof typeof PATHS

export function Icon({ name, size = 18, className }: { name: IconName; size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={className}
    >
      <path d={PATHS[name]} />
    </svg>
  )
}

/** Marque FineMe : trois strates, la dernière en laiton. */
export function Logo({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
      <rect width="32" height="32" rx="9" fill="#2f3fa0" />
      <path d="M9 9.5h14M9 16h10" stroke="#fff" strokeWidth="3" strokeLinecap="round" />
      <path d="M9 22.5h14" stroke="#ddb45e" strokeWidth="3" strokeLinecap="round" />
    </svg>
  )
}

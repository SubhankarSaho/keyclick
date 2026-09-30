export function Logo({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
      <rect x="4" y="14" width="36" height="36" rx="9" fill="#e9ebef" />
      <rect x="18" y="40" width="8" height="2" rx="1" fill="#aeb3bc" />
      <g fill="none" stroke="var(--accent)" strokeWidth="5" strokeLinecap="round">
        <path d="M47 22 A13 13 0 0 1 47 42" />
        <path d="M54 14 A24 24 0 0 1 54 50" opacity="0.6" />
      </g>
    </svg>
  );
}

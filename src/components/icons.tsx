// Iconos SVG propios, trazo de 2 px sobre caja de 24.
type P = { size?: number };
const svg = (size: number, children: React.ReactNode, fill = false) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill={fill ? "currentColor" : "none"} stroke={fill ? "none" : "currentColor"} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    {children}
  </svg>
);

export const MicIcon = ({ size = 22 }: P) =>
  svg(size, <>
    <rect x="9" y="3" width="6" height="11" rx="3" fill="currentColor" />
    <path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21" />
  </>);
export const RecordIcon = ({ size = 20 }: P) =>
  svg(size, <>
    <circle cx="12" cy="12" r="8.5" />
    <circle cx="12" cy="12" r="4" fill="currentColor" />
  </>);
export const StopIcon = ({ size = 14 }: P) => svg(size, <rect x="5" y="5" width="14" height="14" rx="2.5" />, true);
export const NoteIcon = ({ size = 20 }: P) =>
  svg(size, <>
    <path d="M5 5.5A1.5 1.5 0 0 1 6.5 4h11A1.5 1.5 0 0 1 19 5.5V14l-6 6H6.5A1.5 1.5 0 0 1 5 18.5z" />
    <path d="M9 9h6M9 12.5h4M13 20v-4.5a1.5 1.5 0 0 1 1.5-1.5H19" />
  </>);
export const LockIcon = ({ size = 10 }: P) =>
  svg(size, <>
    <rect x="5" y="11" width="14" height="10" rx="2" fill="currentColor" />
    <path d="M8 11V8a4 4 0 0 1 8 0v3" />
  </>);
export const HistoryIcon = ({ size = 18 }: P) =>
  svg(size, <>
    <path d="M3 12a9 9 0 1 0 3-6.7L3 8" />
    <path d="M3 3v5h5M12 7v5l3 2" />
  </>);
export const MeetingIcon = ({ size = 18 }: P) =>
  svg(size, <>
    <rect x="3" y="5" width="18" height="13" rx="2" />
    <path d="M8 21h8M7 10h.01M12 10h5M7 14h10" />
  </>);
export const BookIcon = ({ size = 18 }: P) =>
  svg(size, <>
    <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20V3H6.5A2.5 2.5 0 0 0 4 5.5z" />
    <path d="M4 19.5A2.5 2.5 0 0 0 6.5 22H20v-5" />
  </>);
export const BoltIcon = ({ size = 18 }: P) => svg(size, <path d="M13 2 4 14h7l-1 8 9-12h-7z" />);
export const GearIcon = ({ size = 18 }: P) =>
  svg(size, <>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
  </>);
export const SearchIcon = ({ size = 16 }: P) =>
  svg(size, <>
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-3.5-3.5" />
  </>);
export const CopyIcon = ({ size = 15 }: P) =>
  svg(size, <>
    <rect x="9" y="9" width="12" height="12" rx="2" />
    <path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1" />
  </>);
export const TrashIcon = ({ size = 15 }: P) =>
  svg(size, <path d="M3 6h18M8 6V4h8v2M6 6l1 14h10l1-14" />);
export const CheckIcon = ({ size = 15 }: P) => svg(size, <path d="m5 12 5 5 9-10" />);
export const PlugIcon = ({ size = 18 }: P) =>
  svg(size, <path d="M9 2v6M15 2v6M6 8h12v4a6 6 0 0 1-12 0zM12 18v4" />);

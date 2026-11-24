/** 65_000 → "01:05"; 3_725_000 → "1:02:05". */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

const dayFmt = new Intl.DateTimeFormat("es-ES", { weekday: "long", day: "numeric", month: "long" });
const timeFmt = new Intl.DateTimeFormat("es-ES", { hour: "2-digit", minute: "2-digit" });
const dateTimeFmt = new Intl.DateTimeFormat("es-ES", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

export const formatTime = (ts: number) => timeFmt.format(ts);
export const formatDateTime = (ts: number) => dateTimeFmt.format(ts);

/** "Hoy", "Ayer" o "martes, 18 de noviembre". */
export function formatDay(ts: number): string {
  const d = new Date(ts);
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (d.toDateString() === today.toDateString()) return "Hoy";
  if (d.toDateString() === yesterday.toDateString()) return "Ayer";
  const s = dayFmt.format(d);
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** Agrupa elementos por día conservando el orden. */
export function groupByDay<T>(items: T[], ts: (t: T) => number): [string, T[]][] {
  const groups: [string, T[]][] = [];
  for (const it of items) {
    const day = formatDay(ts(it));
    const last = groups[groups.length - 1];
    if (last && last[0] === day) last[1].push(it);
    else groups.push([day, [it]]);
  }
  return groups;
}

export const countWords = (text: string) => text.trim().split(/\s+/).filter(Boolean).length;

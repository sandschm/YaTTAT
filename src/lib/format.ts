import type { TimeFormat } from "./api";

export function formatDuration(seconds: number, format: TimeFormat = "hoursMinutes"): string {
  const s = Math.max(0, Math.round(seconds));
  if (format === "decimal") {
    return (s / 3600).toFixed(2).replace(".", ",");
  }
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return `${h}:${String(m).padStart(2, "0")}`;
}

/** Duration with unit, e.g. "7:30 h" or "7,50 h". */
export function formatHours(seconds: number, format: TimeFormat = "hoursMinutes"): string {
  return `${formatDuration(seconds, format)} h`;
}

export function formatSigned(seconds: number, format: TimeFormat = "hoursMinutes"): string {
  const sign = seconds < 0 ? "−" : "+";
  return `${sign}${formatDuration(Math.abs(seconds), format)}`;
}

export function formatPercent(part: number, total: number): string {
  if (!total) return "0 %";
  return `${Math.round((part / total) * 100)} %`;
}

export function formatTime(iso: string | Date): string {
  const d = typeof iso === "string" ? new Date(iso) : iso;
  return d.toLocaleTimeString("de-DE", { hour: "2-digit", minute: "2-digit" });
}

export function formatDate(iso: string | Date, opts: Intl.DateTimeFormatOptions = { weekday: "short", day: "2-digit", month: "2-digit", year: "numeric" }): string {
  const d = typeof iso === "string" ? parseDate(iso) : iso;
  return d.toLocaleDateString("de-DE", opts);
}

export function formatDateTime(iso: string | Date): string {
  const d = typeof iso === "string" ? new Date(iso) : iso;
  return d.toLocaleString("de-DE", { day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit" });
}

/** Parses "YYYY-MM-DD" as a local date (not UTC). */
export function parseDate(s: string): Date {
  if (/^\d{4}-\d{2}-\d{2}$/.test(s)) {
    const [y, m, d] = s.split("-").map(Number);
    return new Date(y, m - 1, d);
  }
  return new Date(s);
}

/** "YYYY-MM-DD" of a local date. */
export function toDateKey(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

export function addDays(d: Date, n: number): Date {
  const r = new Date(d);
  r.setDate(r.getDate() + n);
  return r;
}

/** Start of week (firstDay: 0 = Monday … 6 = Sunday). */
export function startOfWeek(d: Date, firstDay = 0): Date {
  const day = (d.getDay() + 6) % 7; // 0 = Monday
  const diff = (day - firstDay + 7) % 7;
  return startOfDay(addDays(d, -diff));
}

export function isoWeek(d: Date): number {
  const date = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  const day = date.getUTCDay() || 7;
  date.setUTCDate(date.getUTCDate() + 4 - day);
  const yearStart = new Date(Date.UTC(date.getUTCFullYear(), 0, 1));
  return Math.ceil(((date.getTime() - yearStart.getTime()) / 86400000 + 1) / 7);
}

export function relativeTime(iso: string | null | undefined): string {
  if (!iso) return "nie";
  const diff = Math.round((Date.now() - new Date(iso).getTime()) / 1000);
  if (diff < 60) return "gerade eben";
  if (diff < 3600) return `vor ${Math.floor(diff / 60)} min`;
  if (diff < 86400) return `vor ${Math.floor(diff / 3600)} h`;
  return formatDate(iso, { day: "2-digit", month: "2-digit", year: "numeric" });
}

export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (!parts.length) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

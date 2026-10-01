const UNITS: { unit: Intl.RelativeTimeFormatUnit; seconds: number }[] = [
  { unit: "year", seconds: 365 * 24 * 3600 },
  { unit: "month", seconds: 30 * 24 * 3600 },
  { unit: "week", seconds: 7 * 24 * 3600 },
  { unit: "day", seconds: 24 * 3600 },
];

const formatter = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

/** "3 weeks ago", "yesterday", "today". Returns null when the date is missing or invalid. */
export function formatAge(iso: string | null | undefined, now = new Date()): string | null {
  const timestamp = iso ? Date.parse(iso) : Number.NaN;
  if (Number.isNaN(timestamp)) return null;
  const elapsedSeconds = Math.max(0, (now.getTime() - timestamp) / 1000);
  const match = UNITS.find(({ seconds }) => elapsedSeconds >= seconds);
  if (!match) return "today";
  return formatter.format(-Math.floor(elapsedSeconds / match.seconds), match.unit);
}

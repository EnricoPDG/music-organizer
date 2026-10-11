/** 215000 → "3:35"; 3_725_000 → "1:02:05". */
export function duration(msValue: number | null | undefined): string {
	if (!msValue || msValue < 0) return '';
	const total = Math.round(msValue / 1000);
	const h = Math.floor(total / 3600);
	const m = Math.floor((total % 3600) / 60);
	const s = String(total % 60).padStart(2, '0');
	return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`;
}

/** "1 track", "12 tracks". */
export function plural(n: number, one: string, many = `${one}s`): string {
	return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

import { attempt, commands, type Settings, type SettingsPatch } from '#lib/api/index.ts';

export const settings = $state<{ value: Settings | null }>({ value: null });

const darkQuery =
	typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null;

/** Apply theme and motion preferences to <html> (see app.css). */
export function applyAppearance(s: Settings | null): void {
	const root = document.documentElement;
	const theme = s?.theme ?? 'system';
	const dark = theme === 'dark' || (theme === 'system' && !!darkQuery?.matches);
	root.dataset.theme = dark ? 'dark' : 'light';
	if (s?.reduceMotion === true) root.dataset.motion = 'reduce';
	else if (s?.reduceMotion === false) root.dataset.motion = 'full';
	else delete root.dataset.motion;
}

darkQuery?.addEventListener('change', () => applyAppearance(settings.value));

export async function loadSettings(): Promise<void> {
	const s = await attempt(commands.getSettings());
	if (s) {
		settings.value = s;
		applyAppearance(s);
	}
	// Re-enable transitions after the theme has been painted.
	requestAnimationFrame(() =>
		requestAnimationFrame(() => document.documentElement.classList.remove('booting'))
	);
}

export async function updateSettings(patch: SettingsPatch): Promise<void> {
	const s = await attempt(commands.updateSettings(patch));
	if (s) {
		settings.value = s;
		applyAppearance(s);
	}
}

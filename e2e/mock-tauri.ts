// A stand-in for the Tauri backend in the browser (mirrors @tauri-apps/api/mocks), so UI flows
// run in Playwright without the Rust app. Tests provide command responses and push events with
// `window.__emit(event, payload)`.
import type { Page } from '@playwright/test';

export type Responses = Record<string, unknown>;

declare global {
	interface Window {
		__responses: Responses;
		__calls: { cmd: string; args: unknown }[];
		__emit: (event: string, payload: unknown) => void;
	}
}

export async function mockTauri(page: Page, responses: Responses): Promise<void> {
	await page.addInitScript((initial: Responses) => {
		const callbacks = new Map<number, (data: unknown) => void>();
		const listeners = new Map<string, number[]>();
		let nextId = 1;
		window.__responses = initial;
		window.__calls = [];
		const w = window as unknown as Record<string, Record<string, unknown>>;
		w.__TAURI_INTERNALS__ = {
			metadata: {
				currentWindow: { label: 'main' },
				currentWebview: { windowLabel: 'main', label: 'main' }
			},
			transformCallback(cb: (d: unknown) => void, once = false) {
				const id = nextId++;
				callbacks.set(id, (d) => {
					if (once) callbacks.delete(id);
					cb?.(d);
				});
				return id;
			},
			unregisterCallback(id: number) {
				callbacks.delete(id);
			},
			convertFileSrc: (p: string) => p,
			async invoke(cmd: string, args: Record<string, unknown>) {
				window.__calls.push({ cmd, args });
				if (cmd === 'plugin:event|listen') {
					const list = listeners.get(args.event as string) ?? [];
					list.push(args.handler as number);
					listeners.set(args.event as string, list);
					return args.handler;
				}
				if (cmd.startsWith('plugin:')) return null;
				const r = window.__responses[cmd];
				if (r && typeof r === 'object' && 'error' in (r as object))
					throw (r as { error: unknown }).error;
				if (r === undefined)
					throw { kind: 'internal', message: `no mock for ${cmd}`, retryable: false };
				return structuredClone(r);
			}
		};
		w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
		window.__emit = (event, payload) => {
			for (const id of listeners.get(event) ?? []) callbacks.get(id)?.({ event, id, payload });
		};
	}, responses);
}

/** Change a command's response from the test. */
export async function respond(page: Page, cmd: string, value: unknown): Promise<void> {
	await page.evaluate(([c, v]) => (window.__responses[c as string] = v), [cmd, value] as const);
}

export const baseResponses: Responses = {
	get_settings: {
		audioFormat: 'm4a',
		namingPattern: '{album_artist}/{album}/{track:02} - {title}',
		theme: 'light',
		reduceMotion: true,
		maxParallelDownloads: 3,
		reviewLowConfidence: true,
		toolPaths: { ytDlp: null, deno: null, ffmpeg: null }
	},
	list_watched_folders: [
		{ id: 'f1', path: '/Users/me/Music', isDownloadTarget: true, lastScanAt: null }
	],
	list_accounts: [],
	list_batches: [],
	list_jobs: [],
	tools_status: { tools: [], ready: true, installing: false, message: null }
};

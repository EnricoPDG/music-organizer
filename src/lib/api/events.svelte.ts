// Typed subscriptions to backend events (contracts/ipc.md, Events). Call from a component's
// script: the listener is removed automatically when the component is destroyed.
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export function onEvent<T>(name: string, handler: (payload: T) => void): void {
	$effect(() => {
		let unlisten: UnlistenFn | undefined;
		let cancelled = false;
		listen<T>(name, (e) => handler(e.payload)).then((fn) => {
			if (cancelled) fn();
			else unlisten = fn;
		});
		return () => {
			cancelled = true;
			unlisten?.();
		};
	});
}

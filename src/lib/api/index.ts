// Typed access to the Rust backend. Commands come from the generated bindings (contracts/ipc.md);
// this layer only adds error handling.
import { commands, type AppError } from '#lib/bindings.ts';
import { toast } from '#lib/stores/toasts.svelte.ts';

export { commands };
export type * from '#lib/bindings.ts';

export function isAppError(e: unknown): e is AppError {
	return typeof e === 'object' && e !== null && 'kind' in e && 'message' in e;
}

export function errorMessage(e: unknown): string {
	if (isAppError(e)) return e.message;
	if (e instanceof Error) return e.message;
	return String(e);
}

/**
 * Run a command and show its error as a toast. Returns `undefined` on failure, so callers can
 * write `const x = await attempt(commands.foo())` and bail out when it's undefined.
 */
export async function attempt<T>(promise: Promise<T>): Promise<T | undefined> {
	try {
		return await promise;
	} catch (e) {
		toast(errorMessage(e), 'bad');
		return undefined;
	}
}

export type ToastTone = 'info' | 'good' | 'bad';
export interface Toast {
	id: number;
	tone: ToastTone;
	message: string;
	action?: { label: string; run: () => void };
}

let nextId = 1;
export const toasts = $state<Toast[]>([]);

export function toast(message: string, tone: ToastTone = 'info', action?: Toast['action']): void {
	const id = nextId++;
	toasts.push({ id, tone, message, action });
	setTimeout(() => dismiss(id), tone === 'bad' ? 8000 : 4000);
}

export function dismiss(id: number): void {
	const i = toasts.findIndex((t) => t.id === id);
	if (i >= 0) toasts.splice(i, 1);
}

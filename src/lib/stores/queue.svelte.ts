// Download queue state (T057). Loads batches and jobs once, then follows `job://progress` and
// `batch://summary`. Updates are applied once per animation frame so a busy queue stays smooth.
import { listen } from '@tauri-apps/api/event';
import {
	attempt,
	commands,
	type BatchSummary,
	type BatchView,
	type JobProgress,
	type JobView
} from '#lib/api/index.ts';

export const queue = $state<{
	batches: BatchView[];
	jobs: Record<string, JobView>;
	loaded: boolean;
}>({ batches: [], jobs: {}, loaded: false });

const FINAL = new Set(['done', 'skipped', 'failed', 'cancelled']);

export function isFinal(state: string): boolean {
	return FINAL.has(state);
}

export function jobsFor(batchId: string): JobView[] {
	return Object.values(queue.jobs).filter((j) => j.batchId === batchId);
}

/** Jobs that are matching, downloading, tagging or finalizing. */
export function activeCount(): number {
	return Object.values(queue.jobs).filter(
		(j) => !isFinal(j.state) && j.state !== 'paused' && j.state !== 'queued'
	).length;
}

export async function refresh(): Promise<void> {
	const [batches, jobs] = await Promise.all([
		attempt(commands.listBatches()),
		attempt(commands.listJobs(null))
	]);
	if (batches) queue.batches = batches;
	if (jobs) queue.jobs = Object.fromEntries(jobs.map((j) => [j.jobId, j]));
	queue.loaded = true;
}

let pendingProgress: JobProgress[] = [];
let pendingSummaries: BatchSummary[] = [];
let frame = 0;
let reloadTimer: ReturnType<typeof setTimeout> | undefined;

function applyPending(): void {
	frame = 0;
	let unknown = false;
	for (const p of pendingProgress) {
		const job = queue.jobs[p.jobId];
		if (!job) {
			unknown = true;
			continue;
		}
		job.state = p.state;
		job.progress = p.progress;
		job.errorKind = p.errorKind;
		job.errorMessage = p.errorMessage;
	}
	for (const s of pendingSummaries) {
		const b = queue.batches.find((x) => x.batchId === s.batchId);
		if (!b) {
			unknown = true;
			continue;
		}
		Object.assign(b, {
			total: s.total,
			done: s.done,
			failed: s.failed,
			skipped: s.skipped,
			active: s.active
		});
	}
	pendingProgress = [];
	pendingSummaries = [];
	// A new import started elsewhere: fetch its rows (debounced).
	if (unknown) {
		clearTimeout(reloadTimer);
		reloadTimer = setTimeout(refresh, 150);
	}
}

function schedule(): void {
	if (!frame) frame = requestAnimationFrame(applyPending);
}

let started = false;

/** Start following queue events (once, from the app shell). */
export async function startQueueSync(): Promise<void> {
	if (started) return;
	started = true;
	await refresh();
	await listen<JobProgress[]>('job://progress', (e) => {
		pendingProgress.push(...e.payload);
		schedule();
	});
	await listen<BatchSummary>('batch://summary', (e) => {
		pendingSummaries.push(e.payload);
		schedule();
	});
}

export const stateLabel: Record<string, string> = {
	queued: 'Waiting',
	matching: 'Finding on YouTube',
	awaiting_review: 'Needs your review',
	downloading: 'Downloading',
	tagging: 'Adding tags',
	finalizing: 'Saving',
	paused: 'Paused',
	done: 'Done',
	skipped: 'Already in library',
	failed: 'Failed',
	cancelled: 'Cancelled'
};

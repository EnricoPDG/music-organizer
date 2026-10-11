// T044: import flow against a mocked backend (contracts/ipc.md).
import { expect, test } from '@playwright/test';
import { baseResponses, mockTauri, respond } from './mock-tauri';

const URL = 'https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M';

const preview = {
	url: URL,
	platform: 'spotify',
	platformName: 'Spotify',
	kind: 'playlist',
	name: "Today's Top Hits",
	owner: 'Spotify',
	artworkUrl: null,
	trackCount: 3,
	unavailableCount: 1,
	fetchMode: 'public_fallback',
	signedIn: false,
	sample: [
		{ title: 'Blinding Lights', artists: ['The Weeknd'], durationMs: 200040 },
		{ title: 'Shape of You', artists: ['Ed Sheeran'], durationMs: 233712 },
		{ title: 'Despacito', artists: ['Luis Fonsi', 'Daddy Yankee'], durationMs: 229360 }
	]
};

const job = (id: string, title: string, artist: string) => ({
	jobId: id,
	batchId: 'b1',
	trackId: `t-${id}`,
	title,
	artists: [artist],
	album: null,
	artworkUrl: null,
	state: 'queued',
	progress: 0,
	errorKind: null,
	errorMessage: null,
	confidenceLevel: null,
	filePath: null
});

test('paste a link, import it and watch the downloads', async ({ page }) => {
	await mockTauri(page, { ...baseResponses, resolve_link: preview });
	await page.goto('/import');

	await page.getByLabel('Playlist link').fill(URL);
	await page.getByRole('button', { name: 'Preview' }).click();

	const card = page.getByRole('region', { name: 'Preview' });
	await expect(card.getByRole('heading', { name: "Today's Top Hits" })).toBeVisible();
	await expect(card.getByText('Best-effort public import')).toBeVisible();
	await expect(card.getByText("1 item can't be imported")).toBeVisible();
	await expect(card.getByText('Luis Fonsi, Daddy Yankee')).toBeVisible();

	await respond(page, 'start_import', {
		batchId: 'b1',
		playlistId: 'p1',
		created: 3,
		existing: 0,
		queued: 3,
		skipped: 0
	});
	await respond(page, 'list_batches', [
		{
			batchId: 'b1',
			name: "Today's Top Hits",
			platform: 'spotify',
			createdAt: '2026-10-11T12:00:00Z',
			finishedAt: null,
			total: 3,
			done: 0,
			failed: 0,
			skipped: 0,
			active: 0
		}
	]);
	await respond(page, 'list_jobs', [
		job('j1', 'Blinding Lights', 'The Weeknd'),
		job('j2', 'Shape of You', 'Ed Sheeran'),
		job('j3', 'Despacito', 'Luis Fonsi')
	]);
	await card.getByRole('button', { name: 'Import and download' }).click();

	await expect(page.getByRole('status').getByText('3 tracks downloading.')).toBeVisible();
	await page.getByRole('button', { name: 'View downloads' }).click();
	await expect(page).toHaveURL(/\/queue$/);
	const list = page.getByRole('list', { name: "Tracks in Today's Top Hits" });
	await expect(list.getByText('Blinding Lights')).toBeVisible();

	// Live progress from the backend.
	await page.evaluate(() =>
		window.__emit('job://progress', [
			{
				jobId: 'j1',
				trackId: 't-j1',
				batchId: 'b1',
				state: 'downloading',
				progress: 0.5,
				errorKind: null,
				errorMessage: null
			}
		])
	);
	const row1 = list.getByRole('listitem').filter({ hasText: 'Blinding Lights' });
	await expect(row1.getByText('Downloading')).toBeVisible();
	await expect(row1.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50');

	// A failure shows its reason and a retry action; the rest keep going.
	await page.evaluate(() => {
		window.__emit('job://progress', [
			{
				jobId: 'j1',
				trackId: 't-j1',
				batchId: 'b1',
				state: 'done',
				progress: 1,
				errorKind: null,
				errorMessage: null
			},
			{
				jobId: 'j2',
				trackId: 't-j2',
				batchId: 'b1',
				state: 'failed',
				progress: 0,
				errorKind: 'unavailable',
				errorMessage: 'This video is no longer available on YouTube. Pick another source.'
			}
		]);
		window.__emit('batch://summary', {
			batchId: 'b1',
			total: 3,
			done: 1,
			failed: 1,
			skipped: 0,
			active: 1
		});
	});
	const row2 = list.getByRole('listitem').filter({ hasText: 'Shape of You' });
	await expect(row2.getByText('no longer available on YouTube')).toBeVisible();
	await expect(row2.getByRole('button', { name: 'Retry Shape of You' })).toBeVisible();
	await expect(row1.getByText('Done')).toBeVisible();
	await expect(page.getByText('1 failed')).toBeVisible();
});

test('a private playlist suggests connecting an account', async ({ page }) => {
	await mockTauri(page, {
		...baseResponses,
		resolve_link: {
			error: {
				kind: 'private',
				message:
					'This playlist is private. Connect your account in Settings → Accounts to import it.',
				retryable: false
			}
		}
	});
	await page.goto('/import');
	await page.getByLabel('Playlist link').fill('https://www.youtube.com/playlist?list=PLprivate');
	await page.keyboard.press('Enter');
	const alert = page.getByRole('alert');
	await expect(alert.getByText('This playlist is private.')).toBeVisible();
	await expect(alert.getByRole('link', { name: 'Open account settings' })).toHaveAttribute(
		'href',
		'/settings/accounts'
	);
});

test('missing tools point to the installer', async ({ page }) => {
	await mockTauri(page, {
		...baseResponses,
		resolve_link: {
			error: {
				kind: 'tool_missing',
				message: "The download tools aren't installed yet.",
				retryable: false
			}
		}
	});
	await page.goto('/import');
	await page.getByLabel('Playlist link').fill(URL);
	await page.keyboard.press('Enter');
	await expect(page.getByRole('link', { name: 'Install download tools' })).toBeVisible();
});

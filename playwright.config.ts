import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
	testDir: 'e2e',
	webServer: {
		command: 'pnpm build && pnpm preview --port 4173 --strictPort',
		port: 4173,
		reuseExistingServer: !process.env.CI
	},
	use: { baseURL: 'http://localhost:4173' },
	// WebKit is what macOS and Linux render the app with; CI runs both.
	projects: [
		{ name: 'chromium', use: { ...devices['Desktop Chrome'] } },
		...(process.env.CI || process.env.E2E_WEBKIT
			? [{ name: 'webkit', use: { ...devices['Desktop Safari'] } }]
			: [])
	]
});

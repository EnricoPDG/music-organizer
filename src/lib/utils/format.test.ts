import { describe, expect, it } from 'vitest';
import { duration, plural } from './format';

describe('duration', () => {
	it('formats minutes and seconds', () => expect(duration(215_000)).toBe('3:35'));
	it('formats hours', () => expect(duration(3_725_000)).toBe('1:02:05'));
	it('is empty for unknown', () => expect(duration(null)).toBe(''));
});

describe('plural', () => {
	it('uses singular for one', () => expect(plural(1, 'track')).toBe('1 track'));
	it('uses plural with locale digits', () =>
		expect(plural(1200, 'track')).toBe(`${(1200).toLocaleString()} tracks`));
});

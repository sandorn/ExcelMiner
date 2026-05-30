import { describe, it, expect } from 'vitest';
import { formatElapsed, timestamp } from '../utils/format';

describe('formatElapsed', () => {
  it('should format seconds', () => {
    expect(formatElapsed(65000)).toBe('1分05秒');
  });
  it('should format zero', () => {
    expect(formatElapsed(0)).toBe('0分00秒');
  });
  it('should format less than a minute', () => {
    expect(formatElapsed(45000)).toBe('0分45秒');
  });
  it('should format several minutes', () => {
    expect(formatElapsed(125000)).toBe('2分05秒');
  });
});

describe('timestamp', () => {
  it('should return HH:MM:SS format', () => {
    expect(timestamp()).toMatch(/^\d{2}:\d{2}:\d{2}$/);
  });
});

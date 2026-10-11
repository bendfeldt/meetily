import { describe, expect, test } from 'bun:test';
import { errorText } from '../../src/lib/utils';

describe('errorText', () => {
  test('uses the string a Tauri command rejects with', () => {
    expect(errorText('database is locked')).toBe('database is locked');
  });

  test('uses the message of an Error', () => {
    expect(errorText(new Error('disk full'))).toBe('disk full');
  });

  test('stringifies other values', () => {
    expect(errorText({ toString: () => 'custom failure' })).toBe('custom failure');
    expect(errorText(42)).toBe('42');
  });

  test('falls back for empty values', () => {
    expect(errorText('')).toBe('Unknown error');
    expect(errorText(new Error(''))).toBe('Unknown error');
    expect(errorText(undefined)).toBe('Unknown error');
    expect(errorText(null)).toBe('Unknown error');
  });
});

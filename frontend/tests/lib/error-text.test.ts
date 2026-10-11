import { describe, expect, test } from 'bun:test';
import { errorText } from '../../src/lib/utils';

describe('errorText', () => {
  test('uses the string a Tauri command rejects with', () => {
    expect(errorText('database is locked')).toBe('database is locked');
  });

  test('uses the message of an Error, even when empty', () => {
    expect(errorText(new Error('disk full'))).toBe('disk full');
    expect(errorText(new Error(''))).toBe('');
  });

  test('uses a string message field on a plain object', () => {
    expect(errorText({ message: 'not found' })).toBe('not found');
  });

  test('describes other values without throwing', () => {
    expect(errorText({ toString: () => 'custom failure' })).toBe('custom failure');
    expect(errorText({ code: 5 })).toBe('{"code":5}');
    expect(errorText(42)).toBe('42');
    const bare = Object.create(null);
    bare.code = 'EBUSY';
    expect(errorText(bare)).toBe('{"code":"EBUSY"}');
  });

  test('falls back when nothing usable is there', () => {
    expect(errorText('')).toBe('Unknown error');
    expect(errorText(undefined)).toBe('Unknown error');
    expect(errorText(null)).toBe('Unknown error');
    expect(errorText({})).toBe('Unknown error');
    expect(errorText(Object.create(null))).toBe('Unknown error');
    const cyclic: Record<string, unknown> = {};
    cyclic.self = cyclic;
    expect(errorText(cyclic)).toBe('Unknown error');
  });

  test('accepts a caller fallback', () => {
    expect(errorText(undefined, 'Unknown error occurred')).toBe('Unknown error occurred');
  });
});

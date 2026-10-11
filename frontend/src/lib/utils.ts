import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/**
 * Detects if an error message indicates that Ollama is not installed or not running
 * @param errorMessage - The error message to check
 * @returns true if the error indicates Ollama is not installed/running
 */
export function isOllamaNotInstalledError(errorMessage: string): boolean {
  if (!errorMessage) return false;

  const lowerError = errorMessage.toLowerCase();

  // Check for common patterns that indicate Ollama is not installed or not running
  const patterns = [
    'cannot connect',
    'connection refused',
    'cli not found',
    'not in path',
    'ollama cli not found',
    'not found or not in path',
    'please check if the server is running',
    'please check if the ollama server is running',
    'econnrefused',
  ];

  return patterns.some(pattern => lowerError.includes(pattern));
}

/**
 * Readable text for a caught error. Tauri commands reject with a plain string,
 * not an Error, so `err instanceof Error` alone loses the cause. Never throws.
 */
export function errorText(err: unknown, fallback = 'Unknown error'): string {
  if (err instanceof Error) return err.message;
  if (typeof err === 'string') return err || fallback;
  if (err == null) return fallback;
  const message = (err as { message?: unknown }).message;
  if (typeof message === 'string' && message) return message;
  try {
    const text = String(err);
    if (text && !text.startsWith('[object ')) return text;
  } catch {
    // e.g. a null-prototype object has no toString
  }
  try {
    const json = JSON.stringify(err);
    if (json && json !== '{}') return json;
  } catch {
    // e.g. a cyclic object
  }
  return fallback;
}

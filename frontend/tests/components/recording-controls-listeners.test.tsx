import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const originalCore = { ...await import('@tauri-apps/api/core') };
const originalEvent = { ...await import('@tauri-apps/api/event') };
const originalAnalytics = { ...await import('../../src/lib/analytics') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('@tauri-apps/api/event', () => originalEvent);
  mock.module('../../src/lib/analytics', () => originalAnalytics);
});

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
let pending: (() => void)[] | null = null; // when set, listen() resolves only on flush
const listen = mock((name: string, handler: Handler) => {
  const register = () => {
    if (!handlers.has(name)) handlers.set(name, new Set());
    handlers.get(name)!.add(handler);
    return () => { handlers.get(name)!.delete(handler); };
  };
  if (!pending) return Promise.resolve(register());
  return new Promise(resolve => pending!.push(() => resolve(register())));
});
const emit = (name: string, payload: unknown) => handlers.get(name)?.forEach(handler => handler({ payload }));
const active = () => [...handlers.values()].reduce((sum, set) => sum + set.size, 0);

mock.module('@tauri-apps/api/event', () => ({ ...originalEvent, listen }));
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke: async () => false }));
mock.module('../../src/lib/analytics', () => ({ default: {
  trackTranscriptionError() {}, trackTranscriptionSuccess() {}, trackButtonClick() {},
} }));
mock.module('../../src/contexts/RecordingStateContext', () => ({
  useRecordingState: () => ({ isPaused: false, isStartingRecording: false }),
}));
const Passthrough = ({ children }: { children?: React.ReactNode }) => <>{children}</>;
mock.module('../../src/components/ui/tooltip', () => ({
  TooltipProvider: Passthrough, Tooltip: Passthrough, TooltipTrigger: Passthrough, TooltipContent: () => null,
}));
const { RecordingControls } = await import('../../src/components/RecordingControls');

let renderer: ReactTestRenderer | undefined;
let stops: unknown[] = [];
let errors: string[] = [];
async function show() {
  // New inline arrows every render, exactly like page.tsx passes them; each
  // render's callbacks write into their own arrays so stale ones are visible.
  const renderStops: unknown[] = (stops = []);
  const renderErrors: string[] = (errors = []);
  const view = (
    <RecordingControls
      isRecording barHeights={[]} isRecordingDisabled={false} isParentProcessing={false}
      onRecordingStart={() => {}} onTranscriptReceived={() => {}}
      onRecordingStop={(callApi = true) => { renderStops.push(callApi); }}
      onTranscriptionError={message => { renderErrors.push(message); }}
    />
  );
  await act(async () => { if (renderer) renderer.update(view); else renderer = create(view); });
}

beforeEach(() => { handlers.clear(); listen.mockClear(); pending = null; });
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
});

describe('RecordingControls event listeners', () => {
  test('registers each listener once across parent re-renders', async () => {
    await show();
    await show();
    await show();
    expect(listen.mock.calls.map(call => call[0]).sort()).toEqual(['speech-detected', 'transcript-error', 'transcription-error']);
    expect(active()).toBe(3);
  });

  test('unmount before listen resolves does not leak listeners', async () => {
    pending = [];
    await show();
    const resolvers = pending;
    await act(async () => renderer!.unmount());
    renderer = undefined;
    while (resolvers.length) await act(async () => { resolvers.shift()!(); });
    expect(active()).toBe(0);
  });

  test('only the latest render callbacks are called', async () => {
    await show();
    const firstErrors = errors;
    await show();
    await act(async () => { emit('transcript-error', 'legacy failure'); });
    expect(firstErrors).toEqual([]);
    expect(errors).toEqual(['legacy failure']);
  });

  test('a per-chunk transcription error does not stop the recording UI', async () => {
    await show();
    const firstStops = stops;
    await show();
    await act(async () => {
      emit('transcription-error', { error: 'chunk failed', userMessage: 'Transcription failed', actionable: false, phase: 'active' });
      emit('transcript-error', 'legacy failure');
    });
    expect(firstStops).toEqual([]);
    expect(stops).toEqual([]);
  });

  test('a fatal engine-init transcription error still stops the recording UI', async () => {
    await show();
    const firstStops = stops;
    await show();
    await act(async () => {
      emit('transcription-error', { error: 'init failed', userMessage: 'Recording failed', actionable: true, phase: 'active' });
    });
    expect(firstStops).toEqual([]);
    expect(stops).toEqual([false]);
  });
});

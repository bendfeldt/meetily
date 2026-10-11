import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create } from 'react-test-renderer';

// Bun shares module mocks between test files; restore application modules after this suite.
const originalOnboarding = { ...await import('../../src/contexts/OnboardingContext') };
afterAll(() => {
  mock.module('../../src/contexts/OnboardingContext', () => originalOnboarding);
});

let invokeError: Error | null = null;
const invoke = mock(async (_command: string, _args?: Record<string, unknown>) => {
  if (invokeError) throw invokeError;
});
mock.module('@tauri-apps/api/core', () => ({ invoke }));
mock.module('../../src/contexts/OnboardingContext', () => ({
  useOnboarding: () => ({
    setPermissionStatus() {},
    setPermissionsSkipped() {},
    completeOnboarding: async () => {},
    permissions: { microphone: 'denied', systemAudio: 'denied' },
  }),
}));
const { PermissionsStep } = await import('../../src/components/onboarding/steps/PermissionsStep');

async function clickBothActions() {
  const renderer = create(<PermissionsStep />);
  const rows = renderer.root.findAll((node) => typeof node.props.onAction === 'function' && typeof node.type !== 'string');
  const action = (title: string) => rows.find((row) => row.props.title === title)!.props.onAction;

  await act(async () => { await action('Microphone')(); });
  await act(async () => { await action('System Audio')(); });
  renderer.unmount();
}

describe('PermissionsStep with denied permissions', () => {
  beforeEach(() => {
    invoke.mockClear();
    invokeError = null;
  });

  test('opens the matching macOS privacy pane for each permission', async () => {
    await clickBothActions();

    expect(invoke.mock.calls).toEqual([
      ['open_system_settings', { preferencePane: 'Privacy_Microphone' }],
      ['open_system_settings', { preferencePane: 'Privacy_ScreenCapture' }],
    ]);
  });

  test('shows manual instructions when System Settings cannot be opened', async () => {
    invokeError = new Error('command open_system_settings not found');
    const originalAlert = globalThis.alert;
    const alert = mock((_message?: string) => {});
    globalThis.alert = alert;
    try {
      await clickBothActions();
    } finally {
      globalThis.alert = originalAlert;
    }

    expect(alert.mock.calls.map(([message]) => message)).toEqual([
      expect.stringContaining('Microphone'),
      expect.stringContaining('Audio Capture'),
    ]);
  });
});

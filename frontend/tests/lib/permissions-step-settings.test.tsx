import { afterAll, describe, expect, mock, test } from 'bun:test';
import { act, create } from 'react-test-renderer';

// Bun shares module mocks between test files; restore application modules after this suite.
const originalOnboarding = { ...await import('../../src/contexts/OnboardingContext') };
afterAll(() => {
  mock.module('../../src/contexts/OnboardingContext', () => originalOnboarding);
});

const invoke = mock(async (_command: string, _args?: Record<string, unknown>) => {});
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

describe('PermissionsStep with denied permissions', () => {
  test('opens the matching macOS privacy pane for each permission', async () => {
    const renderer = create(<PermissionsStep />);
    const rows = renderer.root.findAll((node) => typeof node.props.onAction === 'function' && typeof node.type !== 'string');
    const action = (title: string) => rows.find((row) => row.props.title === title)!.props.onAction;

    await act(async () => { await action('Microphone')(); });
    await act(async () => { await action('System Audio')(); });

    expect(invoke.mock.calls).toEqual([
      ['open_system_settings', { preferencePane: 'Privacy_Microphone' }],
      ['open_system_settings', { preferencePane: 'Privacy_ScreenCapture' }],
    ]);
    renderer.unmount();
  });
});

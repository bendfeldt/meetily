'use client';

import React, { useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useRecordingStop } from '@/hooks/useRecordingStop';
import { useRouter } from 'next/navigation';
import { toast } from 'sonner';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';

/**
 * RecordingPostProcessingProvider
 *
 * This provider handles post-processing when recording stops from any source:
 * - Tray menu stop
 * - Global keyboard shortcut
 * - Overlay stop button
 * - Main UI stop button
 *
 * It listens for the 'recording-stop-complete' event from Rust backend
 * and triggers the full post-processing flow (save to database, navigate, analytics)
 * regardless of which page the user is currently on.
 */
export function RecordingPostProcessingProvider({ children }: { children: React.ReactNode }) {
  // No-op functions since the global RecordingStateContext already handles state updates
  // These are only needed for the hook's local component state management
  const setIsRecording = () => { };
  const setIsRecordingDisabled = () => { };

  const {
    handleRecordingStop,
  } = useRecordingStop(setIsRecording, setIsRecordingDisabled);

  useEffect(() => {
    let unlistenFn: (() => void) | undefined;

    const setupListener = async () => {
      try {
        // Listen for recording-stop-complete event from Rust
        unlistenFn = await listen<boolean>('recording-stop-complete', (event) => {
          console.log('[RecordingPostProcessing] Received recording-stop-complete event:', event.payload);

          // Call the post-processing handler
          // event.payload is the callApi boolean (true for normal stops)
          handleRecordingStop(event.payload);
        });

        console.log('[RecordingPostProcessing] Event listener set up successfully');
      } catch (error) {
        console.error('[RecordingPostProcessing] Failed to set up event listener:', error);
      }
    };

    setupListener();

    return () => {
      if (unlistenFn) {
        console.log('[RecordingPostProcessing] Cleaning up event listener');
        unlistenFn();
      }
    };
  }, [handleRecordingStop]);

  // Fork (bendfeldt/meetily): results of the after-meeting worker for
  // record-only recordings. Latest values live in refs so the listeners are
  // registered once and no event is dropped while re-subscribing.
  const router = useRouter();
  const { refetchMeetings } = useSidebar();
  const { isAutoSummary } = useConfig();
  const { isRecording } = useRecordingState();
  const latest = useRef({ router, refetchMeetings, isAutoSummary, isRecording });
  latest.current = { router, refetchMeetings, isAutoSummary, isRecording };
  useEffect(() => {
    type Payload = { meeting_id: string | null; error?: string };
    const unlisteners = [
      listen<Payload>('meeting-saved', () => { latest.current.refetchMeetings(); }),
      listen<Payload>('meeting-ready', async (event) => {
        const { refetchMeetings, isAutoSummary, isRecording, router } = latest.current;
        await refetchMeetings();
        // Never pull the user off a running recording.
        if (isAutoSummary && !isRecording && event.payload.meeting_id) {
          router.push(`/meeting-details?id=${event.payload.meeting_id}&source=recording`);
        } else {
          toast.success('Meeting transcribed');
        }
      }),
      listen<Payload>('meeting-discarded', async () => {
        await latest.current.refetchMeetings();
        toast.info('Recording had no speech, deleted');
      }),
      listen<Payload>('meeting-transcription-failed', async (event) => {
        await latest.current.refetchMeetings();
        toast.error(event.payload.meeting_id
          ? 'Transcription failed, retry from the meeting page'
          : 'Recording could not be transcribed', {
          description: event.payload.error,
        });
      }),
    ];
    return () => { unlisteners.forEach((p) => p.then((unlisten) => unlisten())); };
  }, []);

  return <>{children}</>;
}

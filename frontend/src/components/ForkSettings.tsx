"use client";

// Fork-only (bendfeldt/meetily): Teams meeting pop-up toggle and Obsidian
// export folder. Stored by the Rust core in fork_settings.json.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { FolderOpen } from 'lucide-react';
import { Switch } from '@/components/ui/switch';
import { usePlatform } from '@/hooks/usePlatform';

interface ForkSettingsData {
  obsidian_export_folder: string | null;
  teams_prompt_enabled: boolean;
}

export function ForkSettings() {
  const platform = usePlatform();
  const [settings, setSettings] = useState<ForkSettingsData | null>(null);

  useEffect(() => {
    invoke<ForkSettingsData>('get_fork_settings')
      .then(setSettings)
      .catch((error) => toast.error('Could not load settings', { description: String(error) }));
  }, []);

  if (!settings) return null;

  const setTeamsPrompt = async (enabled: boolean) => {
    try {
      await invoke('set_teams_prompt_enabled', { enabled });
      setSettings((current) => current && { ...current, teams_prompt_enabled: enabled });
    } catch (error) {
      toast.error('Could not save setting', { description: String(error) });
    }
  };

  const chooseFolder = async () => {
    try {
      const folder = await invoke<string | null>('choose_obsidian_export_folder');
      if (folder) setSettings((current) => current && { ...current, obsidian_export_folder: folder });
    } catch (error) {
      toast.error('Could not save folder', { description: String(error) });
    }
  };

  return (
    <div className="space-y-4">
      {platform === 'macos' && (
        <div className="flex items-center justify-between p-4 border rounded-lg">
          <div className="flex-1">
            <div className="font-medium" id="teams-prompt-label">Ask to record Teams meetings</div>
            <div className="text-sm text-gray-600">
              When Microsoft Teams starts using the microphone, show a pop-up asking whether to record.
              Recordings started from it stop when Teams releases the microphone. Recording other
              people may require telling them.
            </div>
          </div>
          <Switch
            aria-labelledby="teams-prompt-label"
            checked={settings.teams_prompt_enabled}
            onCheckedChange={setTeamsPrompt}
          />
        </div>
      )}

      <div className="p-4 border rounded-lg bg-gray-50">
        <div className="font-medium mb-2">Obsidian export folder</div>
        <div className="text-sm text-gray-600 mb-3 break-all">
          {settings.obsidian_export_folder || 'Not set. Chosen the first time you export a meeting.'}
        </div>
        <button
          onClick={chooseFolder}
          className="flex items-center gap-2 px-3 py-2 text-sm border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
        >
          <FolderOpen className="w-4 h-4" />
          Choose…
        </button>
      </div>
    </div>
  );
}

"use client";

// Fork-only (bendfeldt/meetily): opt-in export of this meeting to an Obsidian
// note. Once exported, the Rust core keeps the note in sync with the summary
// and title. Option/Alt-click chooses a different export folder.

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { FileDown, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';

export function ExportToObsidianButton({ meetingId }: { meetingId: string }) {
  const [isExporting, setIsExporting] = useState(false);

  const handleExport = async (chooseFolder: boolean) => {
    setIsExporting(true);
    try {
      const path = await invoke<string | null>('export_meeting_markdown', { meetingId, chooseFolder });
      if (path) {
        toast.success('Exported to Obsidian', { description: path });
      }
    } catch (error) {
      toast.error('Export to Obsidian failed', {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <Button
      size="sm"
      variant="outline"
      className="px-2 @[22rem]:px-4"
      onClick={(event) => handleExport(event.altKey)}
      disabled={isExporting}
      title="Export to Obsidian (Option-click to choose another folder)"
    >
      {isExporting ? <Loader2 className="animate-spin @[22rem]:mr-2" size={18} /> : <FileDown className="@[22rem]:mr-2" size={18} />}
      <span className="hidden @[22rem]:inline">Obsidian</span>
    </Button>
  );
}

import type { Writable } from "stream"

export interface StoryInfo {
    id: string
    title: string
    dependsOn: string[]
}

export interface DoneStats {
    stories_completed: number
    stories_skipped: number
    total_commits: number
    files_created: number
    files_modified: number
}

export type BaroEvent =
    | { type: "init"; project: string; stories: StoryInfo[] }
    | { type: "dag"; levels: { id: string; title: string }[][] }
    | { type: "story_start"; id: string; title: string }
    | { type: "story_log"; id: string; line: string }
    | {
          type: "story_complete"
          id: string
          duration_secs: number
          files_created: number
          files_modified: number
      }
    | { type: "story_error"; id: string; error: string; attempt: number; max_retries: number }
    | { type: "story_retry"; id: string; attempt: number }
    | { type: "progress"; completed: number; total: number; percentage: number }
    | { type: "push_status"; id: string; success: boolean; error?: string }
    | { type: "done"; total_time_secs: number; stats: DoneStats }

export class BaroEventEmitter {
    constructor(private stream: Writable = process.stdout) {}

    emit(event: BaroEvent): void {
        this.stream.write(JSON.stringify(event) + "\n")
    }
}

import * as fs from "fs"

export interface PrdV2 {
    project: string
    branchName: string
    description: string
    userStories: StoryV2[]
}

export interface StoryV2 {
    id: string
    priority: number
    title: string
    description: string
    dependsOn: string[]
    retries: number
    acceptance: string[]
    tests: string[]
    passes: boolean
    completedAt: string | null
    durationSecs: number | null
}

export function parsePrd(filePath: string): PrdV2 {
    const raw = JSON.parse(fs.readFileSync(filePath, "utf-8"))

    if (!raw.project || !raw.userStories || !Array.isArray(raw.userStories)) {
        throw new Error("Invalid prd.json: missing project or userStories")
    }

    return {
        project: raw.project,
        branchName: raw.branchName ?? "main",
        description: raw.description ?? "",
        userStories: raw.userStories.map((s: any) => ({
            id: s.id,
            priority: s.priority ?? 0,
            title: s.title ?? "",
            description: s.description ?? "",
            dependsOn: s.dependsOn ?? [],
            retries: s.retries ?? 2,
            acceptance: s.acceptance ?? [],
            tests: s.tests ?? [],
            passes: s.passes ?? false,
            completedAt: s.completedAt ?? null,
            durationSecs: s.durationSecs ?? null,
        })),
    }
}

export function getIncompleteStories(prd: PrdV2): StoryV2[] {
    return prd.userStories.filter((s) => !s.passes)
}

export function markStoryComplete(filePath: string, storyId: string, durationSecs: number): void {
    const raw = JSON.parse(fs.readFileSync(filePath, "utf-8"))
    const timestamp = new Date().toISOString()

    for (const story of raw.userStories) {
        if (story.id === storyId) {
            story.passes = true
            story.completedAt = timestamp
            story.durationSecs = durationSecs
        }
    }

    fs.writeFileSync(filePath, JSON.stringify(raw, null, 2) + "\n")
}

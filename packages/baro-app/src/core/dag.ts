import type { StoryV2 } from "./prd.js"

export interface DagLevel {
    stories: StoryV2[]
}

export function buildDag(stories: StoryV2[]): DagLevel[] {
    const incomplete = stories.filter((s) => !s.passes)
    const completedIds = new Set(stories.filter((s) => s.passes).map((s) => s.id))
    const storyMap = new Map(incomplete.map((s) => [s.id, s]))

    const inDegree = new Map<string, number>()
    const dependents = new Map<string, string[]>()

    for (const s of incomplete) {
        // Only count dependencies that are themselves incomplete
        const activeDeps = s.dependsOn.filter(
            (depId) => storyMap.has(depId) && !completedIds.has(depId)
        )
        inDegree.set(s.id, activeDeps.length)

        for (const dep of activeDeps) {
            if (!dependents.has(dep)) dependents.set(dep, [])
            dependents.get(dep)!.push(s.id)
        }
    }

    const levels: DagLevel[] = []
    let queue = incomplete.filter((s) => (inDegree.get(s.id) ?? 0) === 0)

    while (queue.length > 0) {
        // Sort by priority within each level for consistent ordering
        queue.sort((a, b) => a.priority - b.priority)
        levels.push({ stories: [...queue] })

        const nextQueue: StoryV2[] = []
        for (const s of queue) {
            for (const depId of dependents.get(s.id) ?? []) {
                const newDegree = (inDegree.get(depId) ?? 1) - 1
                inDegree.set(depId, newDegree)
                if (newDegree === 0) {
                    const story = storyMap.get(depId)
                    if (story) nextQueue.push(story)
                }
            }
        }

        queue = nextQueue
    }

    // Cycle detection
    const totalInLevels = levels.reduce((sum, l) => sum + l.stories.length, 0)
    if (totalInLevels !== incomplete.length) {
        const placed = new Set(levels.flatMap((l) => l.stories.map((s) => s.id)))
        const cycled = incomplete.filter((s) => !placed.has(s.id)).map((s) => s.id)
        throw new Error(`Dependency cycle detected involving: ${cycled.join(", ")}`)
    }

    return levels
}

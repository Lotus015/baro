import { execSync } from "child_process"

class GitMutex {
    private queue: (() => void)[] = []
    private locked = false

    async acquire(): Promise<void> {
        if (!this.locked) {
            this.locked = true
            return
        }
        return new Promise((resolve) => this.queue.push(resolve))
    }

    release(): void {
        const next = this.queue.shift()
        if (next) {
            next()
        } else {
            this.locked = false
        }
    }
}

export const gitMutex = new GitMutex()

export function getCurrentCommit(cwd: string): string {
    try {
        return execSync("git rev-parse HEAD", { cwd, encoding: "utf-8" }).trim()
    } catch {
        return ""
    }
}

export function getFileStats(
    commitBefore: string,
    cwd: string
): { created: number; modified: number } {
    try {
        let diffCmd: string
        if (commitBefore) {
            diffCmd = `git diff --name-status ${commitBefore} HEAD`
        } else {
            diffCmd = "git diff --name-status 4b825dc642cb6eb9a060e54bf8d69288fbee4904 HEAD"
        }

        const output = execSync(diffCmd, { cwd, encoding: "utf-8" })
        let created = 0
        let modified = 0

        for (const line of output.split("\n")) {
            if (line.startsWith("A\t")) created++
            else if (line.startsWith("M\t")) modified++
        }

        return { created, modified }
    } catch {
        return { created: 0, modified: 0 }
    }
}

export async function autoCommitPrd(storyId: string, cwd: string): Promise<void> {
    await gitMutex.acquire()
    try {
        try {
            execSync("git diff --quiet prd.json", { cwd })
        } catch {
            // prd.json has changes - commit it
            execSync(`git add prd.json && git commit -m "chore: mark ${storyId} as complete"`, {
                cwd,
            })
        }
    } finally {
        gitMutex.release()
    }
}

export function autoPush(cwd: string): { success: boolean; error?: string } {
    try {
        execSync("git remote get-url origin", { cwd, encoding: "utf-8" })
    } catch {
        return { success: false, error: "No remote configured" }
    }

    try {
        const branch = execSync("git branch --show-current", { cwd, encoding: "utf-8" }).trim()
        execSync(`git push origin ${branch}`, { cwd })
        return { success: true }
    } catch (err) {
        return { success: false, error: String(err) }
    }
}

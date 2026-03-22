/**
 * CliTask - spawns a CLI process and captures output.
 */

import { spawn } from "child_process"

export interface CliTaskResult {
    stdout: string
    stderr: string
    exitCode: number
    durationMs: number
}

export interface CliTaskOptions {
    id: string
    command: string
    args: string[]
    cwd: string
    onStdout?: (line: string) => void
    onStderr?: (line: string) => void
}

export class CliTask {
    readonly id: string
    private opts: CliTaskOptions

    constructor(opts: CliTaskOptions) {
        this.id = opts.id
        this.opts = opts
    }

    execute(): Promise<CliTaskResult> {
        return new Promise((resolve, reject) => {
            const start = Date.now()
            let stdout = ""
            let stderr = ""

            const proc = spawn(this.opts.command, this.opts.args, {
                cwd: this.opts.cwd,
                stdio: ["ignore", "pipe", "pipe"],
                env: { ...process.env },
            })

            proc.stdout.on("data", (chunk: Buffer) => {
                const text = chunk.toString()
                stdout += text
                if (this.opts.onStdout) {
                    for (const line of text.split("\n").filter(Boolean)) {
                        this.opts.onStdout(line)
                    }
                }
            })

            proc.stderr.on("data", (chunk: Buffer) => {
                const text = chunk.toString()
                stderr += text
                if (this.opts.onStderr) {
                    for (const line of text.split("\n").filter(Boolean)) {
                        this.opts.onStderr(line)
                    }
                }
            })

            proc.on("error", (err) => {
                reject(new Error(`Failed to spawn ${this.opts.command}: ${err.message}`))
            })

            proc.on("close", (code) => {
                const result: CliTaskResult = {
                    stdout,
                    stderr,
                    exitCode: code ?? 1,
                    durationMs: Date.now() - start,
                }
                if (code === 0) resolve(result)
                else {
                    const err = new Error(`${this.opts.command} exited with code ${code}`)
                    ;(err as any).result = result
                    reject(err)
                }
            })
        })
    }
}

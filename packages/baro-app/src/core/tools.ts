/**
 * Codebase exploration tools for the AI planner.
 * These are passed as function-calling tools to OpenAI.
 * The model decides when and what to read/grep/list.
 */

import * as fs from "fs"
import * as path from "path"
import { execSync } from "child_process"
import type { ToolDef } from "./stream.js"

const IGNORE = new Set([
    "node_modules", ".git", "dist", "build", ".next", ".nuxt",
    "coverage", ".cache", "__pycache__", "target", ".output",
])

const MAX_FILE_SIZE = 15_000

export function createCodebaseTools(cwd: string): ToolDef[] {
    return [
        {
            name: "list_files",
            description: "List files and directories. Use path='' for project root. Returns file names with types. Ignores node_modules, .git, etc.",
            parameters: {
                type: "object",
                properties: {
                    path: { type: "string", description: "Relative path from project root. Empty for root." },
                    recursive: { type: "boolean", description: "List all files recursively (max 200). Default false." },
                },
                required: ["path"],
                additionalProperties: false,
            },
            async invoke(args: { path: string; recursive?: boolean }) {
                const target = safePath(cwd, args.path || ".")
                if (!target || !fs.existsSync(target)) return `Directory not found: ${args.path}`
                if (!fs.statSync(target).isDirectory()) return `Not a directory: ${args.path}`

                const results: string[] = []
                function walk(dir: string, prefix: string, depth: number) {
                    if (results.length >= 200 || depth > 4) return
                    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
                        if (IGNORE.has(entry.name) || entry.name.startsWith(".")) continue
                        const rel = prefix ? `${prefix}/${entry.name}` : entry.name
                        if (entry.isDirectory()) {
                            results.push(rel + "/")
                            if (args.recursive) walk(path.join(dir, entry.name), rel, depth + 1)
                        } else {
                            results.push(rel)
                        }
                    }
                }
                walk(target, "", 0)
                return results.join("\n") || "(empty directory)"
            },
        },
        {
            name: "read_file",
            description: "Read file contents. Large files truncated to ~15000 chars. Use to understand code structure, configs, etc.",
            parameters: {
                type: "object",
                properties: {
                    path: { type: "string", description: "Relative path to file (e.g. 'src/index.ts')" },
                },
                required: ["path"],
                additionalProperties: false,
            },
            async invoke(args: { path: string }) {
                const target = safePath(cwd, args.path)
                if (!target || !fs.existsSync(target)) return `File not found: ${args.path}`
                if (fs.statSync(target).isDirectory()) return `${args.path} is a directory. Use list_files.`
                if (fs.statSync(target).size > 500_000) return `File too large (${(fs.statSync(target).size / 1024).toFixed(0)}KB)`

                let content = fs.readFileSync(target, "utf-8")
                if (content.length > MAX_FILE_SIZE) {
                    content = content.slice(0, MAX_FILE_SIZE) + "\n... (truncated)"
                }
                return content
            },
        },
        {
            name: "grep",
            description: "Search for a text pattern across project files. Returns matching lines with file paths. Ignores node_modules, .git, etc.",
            parameters: {
                type: "object",
                properties: {
                    pattern: { type: "string", description: "Text to search for (case-insensitive)" },
                    path: { type: "string", description: "Directory to search in. Default: entire project." },
                    file_pattern: { type: "string", description: "File glob (e.g. '*.ts'). Default: all." },
                },
                required: ["pattern"],
                additionalProperties: false,
            },
            async invoke(args: { pattern: string; path?: string; file_pattern?: string }) {
                const searchDir = safePath(cwd, args.path || ".")
                if (!searchDir || !fs.existsSync(searchDir)) return `Directory not found: ${args.path}`

                try {
                    const excludes = Array.from(IGNORE).map((d) => `--exclude-dir=${d}`).join(" ")
                    const include = args.file_pattern ? `--include='${args.file_pattern}'` : ""
                    const cmd = `grep -rn -i ${excludes} ${include} --max-count=50 -- ${JSON.stringify(args.pattern)} ${JSON.stringify(searchDir)} 2>/dev/null || true`

                    const output = execSync(cmd, { encoding: "utf-8", maxBuffer: 1024 * 1024 })
                    const lines = output.split("\n").filter(Boolean).map((line) =>
                        line.startsWith(cwd) ? line.slice(cwd.length + 1) : line
                    )
                    return lines.slice(0, 50).join("\n") || "No matches found."
                } catch {
                    return "No matches found."
                }
            },
        },
        {
            name: "file_tree",
            description: "Get a condensed tree view of the project structure up to 3 levels deep. Good starting point to understand the codebase.",
            parameters: {
                type: "object",
                properties: {},
                additionalProperties: false,
            },
            async invoke() {
                const lines: string[] = [path.basename(cwd) + "/"]
                function walk(dir: string, prefix: string, depth: number) {
                    if (lines.length >= 150 || depth > 3) return
                    let entries: fs.Dirent[]
                    try { entries = fs.readdirSync(dir, { withFileTypes: true }) } catch { return }

                    entries.sort((a, b) => {
                        if (a.isDirectory() && !b.isDirectory()) return -1
                        if (!a.isDirectory() && b.isDirectory()) return 1
                        return a.name.localeCompare(b.name)
                    })

                    for (let i = 0; i < entries.length; i++) {
                        if (IGNORE.has(entries[i].name) || entries[i].name.startsWith(".")) continue
                        const isLast = i === entries.length - 1
                        const connector = isLast ? "└── " : "├── "
                        const childPrefix = isLast ? "    " : "│   "
                        if (entries[i].isDirectory()) {
                            lines.push(`${prefix}${connector}${entries[i].name}/`)
                            walk(path.join(dir, entries[i].name), prefix + childPrefix, depth + 1)
                        } else {
                            lines.push(`${prefix}${connector}${entries[i].name}`)
                        }
                    }
                }
                walk(cwd, "", 0)
                return lines.join("\n")
            },
        },
    ]
}

function safePath(cwd: string, filePath: string): string | null {
    const resolved = path.resolve(cwd, filePath)
    if (!resolved.startsWith(path.resolve(cwd))) return null
    return resolved
}

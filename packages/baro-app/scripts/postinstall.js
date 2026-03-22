#!/usr/bin/env node
/**
 * Postinstall script - downloads the baro binary for the current platform.
 * Binary is fetched from GitHub Releases.
 */

import * as fs from "fs"
import * as path from "path"
import { fileURLToPath } from "url"
import * as https from "https"

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const PACKAGE_ROOT = path.resolve(__dirname, "..")
const BIN_DIR = path.join(PACKAGE_ROOT, "bin")
const BINARY_NAME = "baro"
const REPO = "Lotus015/baro"

function getPlatformKey() {
    const platform = process.platform
    const arch = process.arch

    const map = {
        "darwin-arm64": "darwin-arm64",
        "darwin-x64": "darwin-x64",
        "linux-x64": "linux-x64",
        "linux-arm64": "linux-arm64",
    }

    const key = `${platform}-${arch}`
    if (!map[key]) {
        console.warn(`Warning: no prebuilt baro binary for ${key}.`)
        console.warn(`  You can build it manually: cargo build --release -p baro-tui`)
        process.exit(0)
    }
    return map[key]
}

function getVersion() {
    const pkg = JSON.parse(fs.readFileSync(path.join(PACKAGE_ROOT, "package.json"), "utf-8"))
    return pkg.version
}

async function download(url, dest) {
    return new Promise((resolve, reject) => {
        const follow = (url) => {
            https.get(url, { headers: { "User-Agent": "baro-cli" } }, (res) => {
                if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
                    follow(res.headers.location)
                    return
                }
                if (res.statusCode !== 200) {
                    reject(new Error(`Download failed: ${res.statusCode} from ${url}`))
                    return
                }
                const file = fs.createWriteStream(dest)
                res.pipe(file)
                file.on("finish", () => { file.close(); resolve() })
                file.on("error", reject)
            }).on("error", reject)
        }
        follow(url)
    })
}

async function main() {
    const binaryPath = path.join(BIN_DIR, BINARY_NAME)
    if (fs.existsSync(binaryPath)) {
        return
    }

    const platformKey = getPlatformKey()
    const version = getVersion()

    const url = `https://github.com/${REPO}/releases/download/v${version}/${BINARY_NAME}-${platformKey}`

    console.log(`Downloading baro for ${platformKey}...`)

    fs.mkdirSync(BIN_DIR, { recursive: true })

    try {
        await download(url, binaryPath)
        fs.chmodSync(binaryPath, 0o755)
        console.log(`baro installed successfully`)
    } catch (err) {
        console.warn(`Warning: Could not download baro: ${err.message}`)
        console.warn(`  Build manually: cargo build --release -p baro-tui`)
    }
}

main()

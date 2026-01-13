"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const crypto_1 = __importDefault(require("crypto"));
const fs_1 = __importDefault(require("fs"));
const fs_extra_1 = __importDefault(require("fs-extra"));
const os_1 = __importDefault(require("os"));
const path_1 = __importDefault(require("path"));
const chokidar_1 = __importDefault(require("chokidar"));
const ws_1 = require("ws");
const yargs_1 = __importDefault(require("yargs"));
const helpers_1 = require("yargs/helpers");
const pino_1 = __importDefault(require("pino"));
const HOME_ATRIUS = path_1.default.join(os_1.default.homedir(), ".atrius");
const REGISTRY_PATH = path_1.default.join(HOME_ATRIUS, "registry.json");
const DEVICE_PATH = path_1.default.join(HOME_ATRIUS, "device.json");
const VERSIONS_ROOT = path_1.default.join(HOME_ATRIUS, "versions");
const DEFAULT_CHUNK = 64 * 1024;
const logger = (0, pino_1.default)({
    name: "atrius-agent",
    level: process.env.LOG_LEVEL || "info",
});
async function ensureDir(dir) {
    await fs_extra_1.default.ensureDir(dir);
}
async function loadRegistry() {
    if (!(await fs_extra_1.default.pathExists(REGISTRY_PATH))) {
        return {};
    }
    return fs_extra_1.default.readJson(REGISTRY_PATH);
}
async function saveRegistry(registry) {
    await ensureDir(HOME_ATRIUS);
    await fs_extra_1.default.writeJson(REGISTRY_PATH, registry, { spaces: 2 });
}
async function ensureDeviceId() {
    await ensureDir(HOME_ATRIUS);
    if (await fs_extra_1.default.pathExists(DEVICE_PATH)) {
        const stored = await fs_extra_1.default.readJson(DEVICE_PATH);
        if (stored.deviceId)
            return stored.deviceId;
    }
    const deviceId = crypto_1.default.randomUUID();
    await fs_extra_1.default.writeJson(DEVICE_PATH, { deviceId }, { spaces: 2 });
    return deviceId;
}
function computeFileId(filePath) {
    const abs = path_1.default.resolve(filePath);
    return crypto_1.default.createHash("sha256").update(abs).digest("hex").slice(0, 16);
}
function toEnvelope(type, payload) {
    return { type, payload };
}
async function sendFileChunks(socket, fileId, filePath, version, chunkSize) {
    const stats = await fs_extra_1.default.stat(filePath);
    const totalChunks = Math.ceil(stats.size / chunkSize) || 1;
    let seq = 0;
    const stream = fs_1.default.createReadStream(filePath, { highWaterMark: chunkSize });
    for await (const chunk of stream) {
        seq += 1;
        const payload = {
            fileId,
            seq,
            total: totalChunks,
            chunk: Buffer.from(chunk).toString("base64"),
            size: stats.size,
            mtime: stats.mtimeMs,
            version,
        };
        socket.send(JSON.stringify(toEnvelope("file/chunk", payload)));
    }
    const complete = {
        fileId,
        version,
        size: stats.size,
        mtime: stats.mtimeMs,
    };
    socket.send(JSON.stringify(toEnvelope("file/complete", complete)));
}
async function snapshotVersion(fileId, filePath, version) {
    if (!(await fs_extra_1.default.pathExists(filePath)))
        return;
    const targetDir = path_1.default.join(VERSIONS_ROOT, fileId);
    await ensureDir(targetDir);
    const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
    const target = path_1.default.join(targetDir, `${timestamp}-v${version}.bak`);
    await fs_extra_1.default.copyFile(filePath, target);
}
function setupIncomingHandler(socket, registry) {
    const incoming = new Map();
    socket.on("message", async (raw) => {
        const envelope = JSON.parse(typeof raw === "string" ? raw : raw.toString("utf8"));
        if (envelope.type === "file/chunk") {
            const payload = envelope.payload;
            const bucket = incoming.get(payload.fileId) || {
                chunks: [],
                total: payload.total,
                version: payload.version,
                size: payload.size,
            };
            bucket.chunks.push(Buffer.from(payload.chunk, "base64"));
            bucket.total = payload.total;
            bucket.version = payload.version;
            bucket.size = payload.size;
            incoming.set(payload.fileId, bucket);
        }
        if (envelope.type === "file/complete") {
            const payload = envelope.payload;
            const entry = registry[payload.fileId];
            if (!entry)
                return;
            const bucket = incoming.get(payload.fileId);
            if (!bucket)
                return;
            const filePath = entry.path;
            await snapshotVersion(payload.fileId, filePath, entry.version);
            const buffer = Buffer.concat(bucket.chunks);
            await fs_extra_1.default.writeFile(filePath, buffer);
            registry[payload.fileId] = {
                ...entry,
                version: payload.version,
                lastMtime: payload.mtime,
            };
            await saveRegistry(registry);
            incoming.delete(payload.fileId);
            logger.info({ fileId: payload.fileId, version: payload.version }, "applied remote update");
        }
    });
}
async function main() {
    const argv = (0, yargs_1.default)((0, helpers_1.hideBin)(process.argv))
        .option("file", {
        type: "array",
        demandOption: true,
        description: "File(s) to sync",
    })
        .option("relay", {
        type: "string",
        default: "ws://localhost:8787",
        description: "Relay WebSocket URL",
    })
        .option("device", {
        type: "string",
        description: "Optional device label",
    })
        .option("chunk", {
        type: "number",
        default: DEFAULT_CHUNK,
        description: "Chunk size in bytes",
    })
        .option("autoLock", {
        type: "boolean",
        default: true,
        description: "Auto lock while sending changes",
    })
        .strict()
        .parseSync();
    const deviceId = await ensureDeviceId();
    const registry = await loadRegistry();
    const filePaths = argv.file.map((f) => path_1.default.resolve(f));
    const socket = new ws_1.WebSocket(argv.relay);
    socket.on("open", async () => {
        socket.send(JSON.stringify(toEnvelope("device/register", { deviceId, name: argv.device })));
        for (const filePath of filePaths) {
            const fileId = computeFileId(filePath);
            const stats = await fs_extra_1.default.stat(filePath);
            const descriptor = {
                fileId,
                path: filePath,
                size: stats.size,
                mtime: stats.mtimeMs,
            };
            registry[fileId] = {
                path: filePath,
                version: registry[fileId]?.version ?? 0,
                lastMtime: stats.mtimeMs,
            };
            socket.send(JSON.stringify(toEnvelope("file/register", descriptor)));
            const watcher = chokidar_1.default.watch(filePath, {
                ignoreInitial: true,
                awaitWriteFinish: { stabilityThreshold: 300 },
            });
            watcher.on("change", async () => {
                const current = await fs_extra_1.default.stat(filePath);
                const nextVersion = (registry[fileId]?.version ?? 0) + 1;
                if (argv.autoLock) {
                    socket.send(JSON.stringify(toEnvelope("lock/acquire", {
                        fileId,
                        deviceId,
                        mode: "auto",
                    })));
                }
                await sendFileChunks(socket, fileId, filePath, nextVersion, argv.chunk);
                registry[fileId] = {
                    path: filePath,
                    version: nextVersion,
                    lastMtime: current.mtimeMs,
                };
                await saveRegistry(registry);
                if (argv.autoLock) {
                    socket.send(JSON.stringify(toEnvelope("lock/release", {
                        fileId,
                        deviceId,
                        mode: "auto",
                    })));
                }
                logger.info({ fileId, version: nextVersion }, "pushed change");
            });
        }
        setupIncomingHandler(socket, registry);
        logger.info({ relay: argv.relay, files: filePaths, deviceId }, "agent connected and watching files");
    });
    socket.on("close", () => {
        logger.warn("disconnected from relay");
    });
    socket.on("error", (err) => {
        logger.error({ err }, "relay connection error");
    });
}
main().catch((err) => {
    logger.error({ err }, "fatal error");
    process.exit(1);
});

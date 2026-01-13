import crypto from "crypto";
import fs from "fs";
import fse from "fs-extra";
import os from "os";
import path from "path";
import chokidar from "chokidar";
import { WebSocket } from "ws";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import pino from "pino";
import {
  Envelope,
  FileChunkPayload,
  FileCompletePayload,
  FileDescriptor,
} from "../common/types";

type Registry = Record<
  string,
  { path: string; version: number; lastMtime: number }
>;

const HOME_ATRIUS = path.join(os.homedir(), ".atrius");
const REGISTRY_PATH = path.join(HOME_ATRIUS, "registry.json");
const DEVICE_PATH = path.join(HOME_ATRIUS, "device.json");
const VERSIONS_ROOT = path.join(HOME_ATRIUS, "versions");
const DEFAULT_CHUNK = 64 * 1024;

const logger = pino({
  name: "atrius-agent",
  level: process.env.LOG_LEVEL || "info",
});

async function ensureDir(dir: string) {
  await fse.ensureDir(dir);
}

async function loadRegistry(): Promise<Registry> {
  if (!(await fse.pathExists(REGISTRY_PATH))) {
    return {};
  }
  return fse.readJson(REGISTRY_PATH);
}

async function saveRegistry(registry: Registry) {
  await ensureDir(HOME_ATRIUS);
  await fse.writeJson(REGISTRY_PATH, registry, { spaces: 2 });
}

async function ensureDeviceId(): Promise<string> {
  await ensureDir(HOME_ATRIUS);
  if (await fse.pathExists(DEVICE_PATH)) {
    const stored = await fse.readJson(DEVICE_PATH);
    if (stored.deviceId) return stored.deviceId as string;
  }
  const deviceId = crypto.randomUUID();
  await fse.writeJson(DEVICE_PATH, { deviceId }, { spaces: 2 });
  return deviceId;
}

function computeFileId(filePath: string): string {
  const abs = path.resolve(filePath);
  return crypto.createHash("sha256").update(abs).digest("hex").slice(0, 16);
}

function toEnvelope(type: string, payload: unknown): Envelope {
  return { type, payload };
}

async function sendFileChunks(
  socket: WebSocket,
  fileId: string,
  filePath: string,
  version: number,
  chunkSize: number
) {
  const stats = await fse.stat(filePath);
  const totalChunks = Math.ceil(stats.size / chunkSize) || 1;
  let seq = 0;

  const stream = fs.createReadStream(filePath, { highWaterMark: chunkSize });
  for await (const chunk of stream) {
    seq += 1;
    const payload: FileChunkPayload = {
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

  const complete: FileCompletePayload = {
    fileId,
    version,
    size: stats.size,
    mtime: stats.mtimeMs,
  };
  socket.send(JSON.stringify(toEnvelope("file/complete", complete)));
}

async function snapshotVersion(fileId: string, filePath: string, version: number) {
  if (!(await fse.pathExists(filePath))) return;
  const targetDir = path.join(VERSIONS_ROOT, fileId);
  await ensureDir(targetDir);
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  const target = path.join(targetDir, `${timestamp}-v${version}.bak`);
  await fse.copyFile(filePath, target);
}

function setupIncomingHandler(
  socket: WebSocket,
  registry: Registry
) {
  const incoming = new Map<
    string,
    { chunks: Buffer[]; total: number; version: number; size: number }
  >();

  socket.on("message", async (raw) => {
    const envelope: Envelope = JSON.parse(
      typeof raw === "string" ? raw : raw.toString("utf8")
    );

    if (envelope.type === "file/chunk") {
      const payload = envelope.payload as FileChunkPayload;
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
      const payload = envelope.payload as FileCompletePayload;
      const entry = registry[payload.fileId];
      if (!entry) return;
      const bucket = incoming.get(payload.fileId);
      if (!bucket) return;
      const filePath = entry.path;
      await snapshotVersion(payload.fileId, filePath, entry.version);
      const buffer = Buffer.concat(bucket.chunks);
      await fse.writeFile(filePath, buffer);
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
  const argv = yargs(hideBin(process.argv))
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
    .parseSync() as unknown as {
      file: string[];
      relay: string;
      device?: string;
      chunk: number;
      autoLock: boolean;
    };

  const deviceId = await ensureDeviceId();
  const registry = await loadRegistry();
  const filePaths = argv.file.map((f) => path.resolve(f));

  const socket = new WebSocket(argv.relay);

  socket.on("open", async () => {
    socket.send(JSON.stringify(toEnvelope("device/register", { deviceId, name: argv.device })));
    for (const filePath of filePaths) {
      const fileId = computeFileId(filePath);
      const stats = await fse.stat(filePath);
      const descriptor: FileDescriptor = {
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
      const watcher = chokidar.watch(filePath, {
        ignoreInitial: true,
        awaitWriteFinish: { stabilityThreshold: 300 },
      });

      watcher.on("change", async () => {
        const current = await fse.stat(filePath);
        const nextVersion = (registry[fileId]?.version ?? 0) + 1;
        if (argv.autoLock) {
          socket.send(
            JSON.stringify(
              toEnvelope("lock/acquire", {
                fileId,
                deviceId,
                mode: "auto",
              })
            )
          );
        }
        await sendFileChunks(socket, fileId, filePath, nextVersion, argv.chunk);
        registry[fileId] = {
          path: filePath,
          version: nextVersion,
          lastMtime: current.mtimeMs,
        };
        await saveRegistry(registry);
        if (argv.autoLock) {
          socket.send(
            JSON.stringify(
              toEnvelope("lock/release", {
                fileId,
                deviceId,
                mode: "auto",
              })
            )
          );
        }
        logger.info({ fileId, version: nextVersion }, "pushed change");
      });
    }

    setupIncomingHandler(socket, registry);
    logger.info(
      { relay: argv.relay, files: filePaths, deviceId },
      "agent connected and watching files"
    );
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


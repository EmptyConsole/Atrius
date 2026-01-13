import { WebSocketServer, WebSocket } from "ws";
import { v4 as uuid } from "uuid";
import pino from "pino";
import {
  DeviceRegistration,
  DeviceSession,
  Envelope,
  ErrorPayload,
  FileChunkPayload,
  FileCompletePayload,
  FileDescriptor,
  FileState,
  LockPayload,
  LockStatePayload,
} from "../common/types";

const PORT = parseInt(process.env.ATRIUS_RELAY_PORT || "8787", 10);
const log = pino({
  name: "atrius-relay",
  level: process.env.LOG_LEVEL || "info",
});

const devices = new Map<string, DeviceSession>();
const files = new Map<string, FileState>();

function safeSend(socket: WebSocket, message: Envelope) {
  if (socket.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify(message));
  }
}

function broadcastToFile(fileId: string, message: Envelope, exclude?: string) {
  const state = files.get(fileId);
  if (!state) return;
  state.members.forEach((memberId) => {
    if (memberId === exclude) return;
    const target = devices.get(memberId);
    if (target) {
      safeSend(target.socket, message);
    }
  });
}

function upsertFile(file: FileDescriptor, deviceId: string): FileState {
  let state = files.get(file.fileId);
  if (!state) {
    state = { fileId: file.fileId, members: new Set(), version: 0 };
    files.set(file.fileId, state);
  }
  state.members.add(deviceId);
  return state;
}

function handleDeviceRegister(
  socket: WebSocket,
  payload: DeviceRegistration
): string {
  const deviceId = payload.deviceId || uuid();
  devices.set(deviceId, { id: deviceId, name: payload.name, socket });
  safeSend(socket, { type: "device/registered", payload: { deviceId } });
  log.info({ deviceId, name: payload.name }, "device registered");
  return deviceId;
}

function handleFileRegister(
  deviceId: string,
  socket: WebSocket,
  payload: FileDescriptor
) {
  const state = upsertFile(payload, deviceId);
  const members = Array.from(state.members);
  safeSend(socket, {
    type: "file/registered",
    payload: { fileId: payload.fileId, members, version: state.version },
  });
  broadcastToFile(
    payload.fileId,
    { type: "presence/update", payload: { fileId: payload.fileId, members } },
    deviceId
  );
}

function handleLockChange(
  deviceId: string,
  payload: LockPayload,
  acquire: boolean
) {
  const state = files.get(payload.fileId);
  if (!state) return;
  if (acquire) {
    if (state.lockOwner && state.lockOwner !== deviceId) {
      return;
    }
    state.lockOwner = deviceId;
  } else if (state.lockOwner === deviceId) {
    state.lockOwner = undefined;
  }
  const lockPayload: LockStatePayload = {
    fileId: payload.fileId,
    owner: state.lockOwner,
  };
  broadcastToFile(payload.fileId, { type: "lock/state", payload: lockPayload });
}

function handleChunk(
  deviceId: string,
  payload: FileChunkPayload & { fileId: string }
) {
  const state = files.get(payload.fileId);
  if (!state) return;
  broadcastToFile(
    payload.fileId,
    { type: "file/chunk", payload },
    deviceId
  );
}

function handleComplete(
  deviceId: string,
  payload: FileCompletePayload & { fileId: string }
) {
  const state = files.get(payload.fileId);
  if (!state) return;
  state.version = Math.max(state.version, payload.version);
  broadcastToFile(
    payload.fileId,
    { type: "file/complete", payload },
    deviceId
  );
}

function cleanupDevice(deviceId: string | null) {
  if (!deviceId) return;
  devices.delete(deviceId);
  files.forEach((state) => {
    state.members.delete(deviceId);
    if (state.lockOwner === deviceId) {
      state.lockOwner = undefined;
    }
  });
}

function parseEnvelope(raw: WebSocket.RawData): Envelope | null {
  try {
    const text = typeof raw === "string" ? raw : raw.toString("utf8");
    return JSON.parse(text);
  } catch (err) {
    log.warn({ err }, "failed to parse envelope");
    return null;
  }
}

const server = new WebSocketServer({ port: PORT });

server.on("connection", (socket) => {
  let deviceId: string | null = null;

  socket.on("message", (raw) => {
    const envelope = parseEnvelope(raw);
    if (!envelope) return;

    switch (envelope.type) {
      case "device/register":
        deviceId = handleDeviceRegister(socket, envelope.payload as DeviceRegistration);
        break;
      case "file/register":
        if (!deviceId) return;
        handleFileRegister(deviceId, socket, envelope.payload as FileDescriptor);
        break;
      case "lock/acquire":
        if (!deviceId) return;
        handleLockChange(deviceId, envelope.payload as LockPayload, true);
        break;
      case "lock/release":
        if (!deviceId) return;
        handleLockChange(deviceId, envelope.payload as LockPayload, false);
        break;
      case "file/chunk":
        if (!deviceId) return;
        handleChunk(deviceId, envelope.payload as FileChunkPayload & { fileId: string });
        break;
      case "file/complete":
        if (!deviceId) return;
        handleComplete(deviceId, envelope.payload as FileCompletePayload & { fileId: string });
        break;
      case "file/error":
        log.warn({ payload: envelope.payload as ErrorPayload }, "file error");
        break;
      default:
        log.warn({ type: envelope.type }, "unknown envelope type");
    }
  });

  socket.on("close", () => {
    cleanupDevice(deviceId);
  });
});

log.info(`Atrius relay listening on ws://localhost:${PORT}`);


"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const ws_1 = require("ws");
const uuid_1 = require("uuid");
const pino_1 = __importDefault(require("pino"));
const PORT = parseInt(process.env.ATRIUS_RELAY_PORT || "8787", 10);
const log = (0, pino_1.default)({
    name: "atrius-relay",
    level: process.env.LOG_LEVEL || "info",
});
const devices = new Map();
const files = new Map();
function safeSend(socket, message) {
    if (socket.readyState === ws_1.WebSocket.OPEN) {
        socket.send(JSON.stringify(message));
    }
}
function broadcastToFile(fileId, message, exclude) {
    const state = files.get(fileId);
    if (!state)
        return;
    state.members.forEach((memberId) => {
        if (memberId === exclude)
            return;
        const target = devices.get(memberId);
        if (target) {
            safeSend(target.socket, message);
        }
    });
}
function upsertFile(file, deviceId) {
    let state = files.get(file.fileId);
    if (!state) {
        state = { fileId: file.fileId, members: new Set(), version: 0 };
        files.set(file.fileId, state);
    }
    state.members.add(deviceId);
    return state;
}
function handleDeviceRegister(socket, payload) {
    const deviceId = payload.deviceId || (0, uuid_1.v4)();
    devices.set(deviceId, { id: deviceId, name: payload.name, socket });
    safeSend(socket, { type: "device/registered", payload: { deviceId } });
    log.info({ deviceId, name: payload.name }, "device registered");
    return deviceId;
}
function handleFileRegister(deviceId, socket, payload) {
    const state = upsertFile(payload, deviceId);
    const members = Array.from(state.members);
    safeSend(socket, {
        type: "file/registered",
        payload: { fileId: payload.fileId, members, version: state.version },
    });
    broadcastToFile(payload.fileId, { type: "presence/update", payload: { fileId: payload.fileId, members } }, deviceId);
}
function handleLockChange(deviceId, payload, acquire) {
    const state = files.get(payload.fileId);
    if (!state)
        return;
    if (acquire) {
        if (state.lockOwner && state.lockOwner !== deviceId) {
            return;
        }
        state.lockOwner = deviceId;
    }
    else if (state.lockOwner === deviceId) {
        state.lockOwner = undefined;
    }
    const lockPayload = {
        fileId: payload.fileId,
        owner: state.lockOwner,
    };
    broadcastToFile(payload.fileId, { type: "lock/state", payload: lockPayload });
}
function handleChunk(deviceId, payload) {
    const state = files.get(payload.fileId);
    if (!state)
        return;
    broadcastToFile(payload.fileId, { type: "file/chunk", payload }, deviceId);
}
function handleComplete(deviceId, payload) {
    const state = files.get(payload.fileId);
    if (!state)
        return;
    state.version = Math.max(state.version, payload.version);
    broadcastToFile(payload.fileId, { type: "file/complete", payload }, deviceId);
}
function cleanupDevice(deviceId) {
    if (!deviceId)
        return;
    devices.delete(deviceId);
    files.forEach((state) => {
        state.members.delete(deviceId);
        if (state.lockOwner === deviceId) {
            state.lockOwner = undefined;
        }
    });
}
function parseEnvelope(raw) {
    try {
        const text = typeof raw === "string" ? raw : raw.toString("utf8");
        return JSON.parse(text);
    }
    catch (err) {
        log.warn({ err }, "failed to parse envelope");
        return null;
    }
}
const server = new ws_1.WebSocketServer({ port: PORT });
server.on("connection", (socket) => {
    let deviceId = null;
    socket.on("message", (raw) => {
        const envelope = parseEnvelope(raw);
        if (!envelope)
            return;
        switch (envelope.type) {
            case "device/register":
                deviceId = handleDeviceRegister(socket, envelope.payload);
                break;
            case "file/register":
                if (!deviceId)
                    return;
                handleFileRegister(deviceId, socket, envelope.payload);
                break;
            case "lock/acquire":
                if (!deviceId)
                    return;
                handleLockChange(deviceId, envelope.payload, true);
                break;
            case "lock/release":
                if (!deviceId)
                    return;
                handleLockChange(deviceId, envelope.payload, false);
                break;
            case "file/chunk":
                if (!deviceId)
                    return;
                handleChunk(deviceId, envelope.payload);
                break;
            case "file/complete":
                if (!deviceId)
                    return;
                handleComplete(deviceId, envelope.payload);
                break;
            case "file/error":
                log.warn({ payload: envelope.payload }, "file error");
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

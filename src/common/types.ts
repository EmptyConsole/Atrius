import type WebSocket from "ws";

export interface Envelope<T extends string = string, P = unknown> {
  type: T;
  payload: P;
}

export interface DeviceRegistration {
  deviceId?: string;
  name?: string;
}

export interface DeviceRegistered {
  deviceId: string;
}

export interface FileDescriptor {
  fileId: string;
  path: string;
  size: number;
  mtime: number;
}

export interface FileChunkPayload {
  fileId: string;
  seq: number;
  total: number;
  chunk: string; // base64
  size: number;
  mtime: number;
  version: number;
}

export interface FileCompletePayload {
  fileId: string;
  version: number;
  size: number;
  mtime: number;
}

export interface LockPayload {
  fileId: string;
  deviceId: string;
  mode: "auto" | "manual";
}

export interface LockStatePayload {
  fileId: string;
  owner?: string;
}

export interface ErrorPayload {
  fileId?: string;
  message: string;
}

export interface DeviceSession {
  id: string;
  name?: string;
  socket: WebSocket;
}

export interface FileState {
  fileId: string;
  members: Set<string>;
  lockOwner?: string;
  version: number;
}


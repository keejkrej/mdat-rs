export interface DatasetInfo {
  n_pos: number;
  n_time: number;
  n_chan: number;
  n_z: number;
}

export interface Channel {
  index: number;
  name: string;
  color: string;
}

export interface DatasetMetadata {
  normalized: unknown;
  raw?: string;
  raw_format?: string;
}

export interface Dataset {
  name: string;
  info: DatasetInfo;
  width: number;
  height: number;
  channels: Channel[];
  metadata: DatasetMetadata;
}

/** Per-channel viewer state (mutable, persists across position switches). */
export interface ChannelState {
  index: number;
  name: string;
  color: string;
  visible: boolean;
  contrastMin: number;
  contrastMax: number;
}

/** A plane address. */
export interface PlaneAddr {
  p: number;
  t: number;
  c: number;
  z: number;
}

/** A fetched u16 plane. */
export type Plane = Uint16Array;

/** Viewer-level error. `fatal` means the server is likely gone (show Reload). */
export interface ViewerError {
  message: string;
  fatal: boolean;
  frame?: PlaneAddr | null;
}
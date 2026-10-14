export type Lang = "en" | "zh";
export interface Brief {
  topic: string;
  direction: string;
  audience: string;
  platform: string;
  language: string;
  duration: number;
  aspect: string;
}
export interface Frame {
  time: number;
  path: string;
}
export interface Media {
  id: string;
  path: string;
  name: string;
  kind: string;
  role: string;
  duration: number;
  width: number;
  height: number;
  hasAudio: boolean;
  bytes: number;
  modified: number;
  frames: Frame[];
  transcript: string | null;
  warning: string | null;
}
export interface Skill {
  id: string;
  name: string;
  description: string;
  instructions: string;
  origin: string;
}
export interface Clip {
  sourceId: string;
  start: number;
  end: number;
  beat: string;
  reason: string;
  caption: string;
}
export interface Plan {
  title: string;
  summary: string;
  warnings: string[];
  clips: Clip[];
}
export interface Beat {
  start: number;
  end: number;
  purpose: string;
  visual: string;
  script?: string;
  spoken?: string;
  onScreen?: string;
  audio: string;
  editing?: string;
  evidence?: string;
}
export interface Result {
  title?: string;
  summary?: string;
  hook?: string;
  visualStyle?: string;
  pacing?: string;
  audio?: string;
  transferableRules?: string[];
  warnings?: string[];
  beats?: Beat[];
  scenes?: Beat[];
  clips?: Clip[];
  video?: string;
  captions?: string;
  duration?: number;
  width?: number;
  height?: number;
  clipCount?: number;
  encoder?: string;
  hardwareEncoding?: boolean;
  hardwareScaling?: boolean;
  _inputRevision?: number;
}
export interface Artifact {
  id: string;
  kind: string;
  title: string;
  path: string;
  created: number;
  runId: string;
  data: Result;
}
export interface Run {
  id: string;
  stage: string;
  status: string;
  started: number;
  finished: number | null;
  message: string;
  logs: string[];
  artifactId: string | null;
  inputRevision: number;
}
export interface Workspace {
  schemaVersion: number;
  id: string;
  name: string;
  root: string;
  created: number;
  updated: number;
  revision: number;
  brief: Brief;
  reference: Media | null;
  rawFolder: string | null;
  media: Media[];
  skills: Skill[];
  selectedSkills: string[];
  artifacts: Artifact[];
  runs: Run[];
}
export interface Recent {
  id: string;
  name: string;
  root: string;
  updated: number;
}
export interface GenerationSettings {
  voice: string;
  voiceModel: string;
  speechRate: number;
  emotion: string;
  imageProvider: string;
  imageModel: string;
  imageSize: string;
  googleProject: string;
}
export interface Settings {
  mode: string;
  baseUrl: string;
  model: string;
  claudePath: string;
  ffmpegPath: string;
  ffprobePath: string;
  whisperPath: string;
  generation: GenerationSettings;
}
export interface AppData {
  settings: Settings;
  recents: Recent[];
}
export interface Diagnostics {
  claude: string | null;
  ffmpeg: string | null;
  ffprobe: string | null;
  whisper: string | null;
  keySaved: boolean;
  errors: string[];
}

export interface CliStatus {
  path: string | null;
  version: string | null;
  loggedIn: boolean;
  authMethod: string | null;
  account: string | null;
  checked: number;
  error: string | null;
}

export interface AdcStatus {
  loggedIn: boolean;
  project: string | null;
  checked: number;
  expiresAt: number | null;
  error: string | null;
}

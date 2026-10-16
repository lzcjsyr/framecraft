export interface Brief {
  topic: string;
  direction: string;
  audience: string;
  platform: string;
  language: string;
  duration: number;
  aspect: string;
  narration: boolean;
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
  frames: { time: number; path: string }[];
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
export interface Artifact {
  id: string;
  kind: string;
  title: string;
  path: string;
  created: number;
  runId: string;
  data: {
    video?: string;
    captions?: string;
    summary?: string;
    warnings?: string[];
    duration?: number;
    width?: number;
    height?: number;
  };
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
  baseArtifactId: string | null;
}
export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  text: string;
  runId: string;
  created: number;
  artifactId: string | null;
}
export interface CliStatus {
  loggedIn: boolean;
  account: string | null;
  version: string | null;
  error: string | null;
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
  assetsFolder: string | null;
  media: Media[];
  skills: Skill[];
  selectedSkills: string[];
  artifacts: Artifact[];
  runs: Run[];
  conversation: ChatMessage[];
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
export interface Recent {
  id: string;
  name: string;
  root: string;
  updated: number;
}
export interface AppData {
  settings: Settings;
  recents: Recent[];
}
export interface AdcStatus {
  loggedIn: boolean;
  project: string | null;
  checked: number;
  error: string | null;
}
export interface Diagnostics {
  claude: string | null;
  ffmpeg: string | null;
  ffprobe: string | null;
  keySaved: boolean;
  errors: string[];
}

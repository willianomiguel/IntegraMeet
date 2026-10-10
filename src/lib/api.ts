// Tipos e chamadas ao núcleo Rust. Os nomes seguem o JSON gerado pelo serde (camelCase).
import { invoke } from "@tauri-apps/api/core";

export type Profile = "leve" | "balanceado" | "alta";
export type ModelKind = "transcription" | "intelligence";

export interface Settings {
  onboardingCompleted: boolean;
  onboardingStep: number;
  profile: Profile | null;
  transcriptionModel: string | null;
  intelligenceModel: string | null;
  recordingsDir: string | null;
  modelsDir: string | null;
  microphoneId: string | null;
  captureSystemAudio: boolean;
}

export interface GpuInfo {
  name: string;
  vramMb: number;
  vramFreeMb: number;
}

export interface HardwareInfo {
  osCaption: string;
  osBuild: number;
  windowsSupported: boolean;
  cpuName: string;
  cpuThreads: number;
  ramGb: number;
  gpus: GpuInfo[];
  diskFreeGb: number;
  vulkanAvailable: boolean;
}

export interface HardwareReport {
  info: HardwareInfo;
  recommended: Profile;
}

export interface ModelView {
  id: string;
  kind: ModelKind;
  title: string;
  purpose: string;
  requirement: string;
  sizeBytes: number;
  installed: boolean;
  downloading: boolean;
  active: boolean;
}

export interface ModelProgress {
  modelId: string;
  fileName: string;
  fileIndex: number;
  fileCount: number;
  downloaded: number;
  total: number;
  speedBps: number;
  state: "baixando" | "verificando" | "concluido";
}

export type StreamStatus =
  | { estado: "iniciando" | "ativo" | "parado" }
  | { estado: "reconectando" | "falhou"; mensagem: string };

export interface AudioDevice {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface AudioDevices {
  microphones: AudioDevice[];
  outputName: string | null;
}

export interface TestReading {
  level: number;
  status: StreamStatus;
  deviceName: string;
  elapsedMs: number;
}

export interface TestResult {
  path: string;
  durationMs: number;
  peak: number;
}

export interface RecordingStatus {
  meetingId: string;
  title: string;
  startedAt: string;
  elapsedMs: number;
  paused: boolean;
  sources: { source: "microfone" | "sistema"; deviceName: string; status: StreamStatus; level: number }[];
  problem: string | null;
}

export type MeetingStatus =
  | "gravando"
  | "gravacao_interrompida"
  | "pronta"
  | "transcrevendo"
  | "transcrita"
  | "analisando"
  | "analisada"
  | "erro";

export interface MeetingSummary {
  id: string;
  title: string;
  startedAt: string;
  durationMs: number;
  status: MeetingStatus;
}

export interface Segment {
  id: number;
  source: string;
  startMs: number;
  endMs: number;
  text: string;
  confidence: number | null;
}

export type ItemKind = "decisao" | "tarefa" | "pendencia" | "pergunta" | "risco" | "momento";

export interface AnalysisItem {
  id: number;
  kind: ItemKind;
  text: string;
  owner: string | null;
  due: string | null;
  segmentIds: number[];
}

export interface Analysis {
  createdAt: string;
  model: string;
  summary: string;
  topics: string[];
  items: AnalysisItem[];
}

export interface MeetingDetail {
  id: string;
  title: string;
  startedAt: string;
  endedAt: string | null;
  durationMs: number;
  status: MeetingStatus;
  error: string | null;
  folder: string;
  recordings: { source: string; deviceName: string; filePath: string; offsetMs: number }[];
  segments: Segment[];
  analysis: Analysis | null;
}

export interface SearchHit {
  meetingId: string;
  meetingTitle: string;
  startedAt: string;
  snippet: string;
  segmentId: number | null;
  startMs: number | null;
}

export interface ProcessingItem {
  meetingId: string;
  title: string;
  kind: string;
  status: string;
  progress: number | null;
}

export interface StoragePaths {
  recordings: string;
  models: string;
  database: string;
  logs: string;
  config: string;
}

export interface LlamaState {
  estado: string;
  mensagem?: string;
}

export const api = {
  appInfo: () => invoke<{ name: string; version: string; buildDate: string; dataDir: string; platform: string }>("app_info"),
  hardware: () => invoke<HardwareReport>("hardware_report"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveOnboardingStep: (step: number) => invoke<void>("save_onboarding_step", { step }),
  setProfile: (profile: Profile) => invoke<Settings>("set_profile", { profile }),
  completeOnboarding: () => invoke<Settings>("complete_onboarding"),
  modelCatalog: () => invoke<ModelView[]>("model_catalog"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  cancelModelDownload: (id: string) => invoke<void>("cancel_model_download", { id }),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  llamaStatus: () => invoke<LlamaState>("llama_status"),

  audioDevices: () => invoke<AudioDevices>("audio_devices"),
  audioTestStart: (source: "microfone" | "sistema", micId: string | null) =>
    invoke<void>("audio_test_start", { source, micId }),
  audioTestRead: () => invoke<TestReading | null>("audio_test_read"),
  audioTestStop: (source: "microfone" | "sistema") => invoke<TestResult>("audio_test_stop", { source }),
  audioTestTranscribe: (path: string) => invoke<string>("audio_test_transcribe", { path }),

  recordingStart: (title: string, micId: string | null, includeSystem: boolean) =>
    invoke<string>("recording_start", { title, micId, includeSystem }),
  recordingStatus: () => invoke<RecordingStatus | null>("recording_status"),
  recordingPause: (paused: boolean) => invoke<void>("recording_pause", { paused }),
  recordingStop: () => invoke<string>("recording_stop"),

  meetingsList: () => invoke<MeetingSummary[]>("meetings_list"),
  meetingDetail: (id: string) => invoke<MeetingDetail>("meeting_detail", { id }),
  meetingRename: (id: string, title: string) => invoke<void>("meeting_rename", { id, title }),
  meetingDelete: (id: string) => invoke<void>("meeting_delete", { id }),
  meetingTranscribe: (id: string) => invoke<void>("meeting_transcribe", { id }),
  meetingAnalyze: (id: string) => invoke<void>("meeting_analyze", { id }),
  meetingCancel: (id: string) => invoke<void>("meeting_cancel", { id }),
  processingStatus: () => invoke<ProcessingItem[]>("processing_status"),
  search: (query: string) => invoke<SearchHit[]>("search_meetings", { query }),

  setMicrophone: (micId: string | null) => invoke<Settings>("set_microphone", { micId }),
  setCaptureSystem: (enabled: boolean) => invoke<Settings>("set_capture_system", { enabled }),
  setTranscriptionModel: (id: string) => invoke<Settings>("set_transcription_model", { id }),
  setIntelligenceModel: (id: string) => invoke<Settings>("set_intelligence_model", { id }),
  setRecordingsDir: (path: string | null) => invoke<Settings>("set_recordings_dir", { path }),
  setModelsDir: (path: string | null) => invoke<Settings>("set_models_dir", { path }),
  storagePaths: () => invoke<StoragePaths>("storage_paths"),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  exportDiagnostics: () => invoke<string>("export_diagnostics"),
};

/** Mensagem amigável a partir do erro que o núcleo devolve (já vem em português). */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Algo não saiu como esperado. Tente novamente.";
}

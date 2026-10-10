import { useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { api, AudioDevice, errorText, ModelKind, ModelProgress, ModelView, Settings, StoragePaths } from "../lib/api";
import { formatBytes, formatSpeed } from "../lib/format";
import { Badge, Button, Card, Notice, ProgressBar } from "../components/ui";

interface Props {
  settings: Settings;
  onSettings: (s: Settings) => void;
}

export default function SettingsScreen({ settings, onSettings }: Props) {
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const run = async (fn: () => Promise<unknown>, ok?: string) => {
    setError(null);
    setNotice(null);
    try {
      await fn();
      if (ok) setNotice(ok);
    } catch (e) {
      setError(errorText(e));
    }
  };

  return (
    <div className="flex flex-col gap-8">
      <header>
        <h1 className="text-2xl font-semibold tracking-tight">Configurações</h1>
        <p className="text-sm text-[var(--muted)]">Tudo fica neste computador. Ajuste apenas o que precisar.</p>
      </header>

      {error && <Notice tone="error">{error}</Notice>}
      {notice && <Notice tone="ok">{notice}</Notice>}

      <ModelsSection onSettings={onSettings} onError={setError} />
      <AudioSection settings={settings} onSettings={onSettings} onError={setError} />
      <FoldersSection settings={settings} onSettings={onSettings} run={run} />
      <DiagnosticsSection onError={setError} onNotice={setNotice} />
      <AboutSection />
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-lg font-medium">{title}</h2>
      {children}
    </section>
  );
}

function ModelsSection({ onSettings, onError }: { onSettings: (s: Settings) => void; onError: (m: string | null) => void }) {
  const [models, setModels] = useState<ModelView[]>([]);
  const [progress, setProgress] = useState<Record<string, ModelProgress>>({});

  const reload = () => api.modelCatalog().then(setModels).catch((e) => onError(errorText(e)));

  useEffect(() => {
    reload();
    const unsub = Promise.all([
      listen<ModelProgress>("model-progress", (e) => setProgress((p) => ({ ...p, [e.payload.modelId]: e.payload }))),
      listen<string>("model-finished", () => reload()),
      listen<{ id: string; message: string }>("model-failed", (e) => {
        onError(e.payload.message);
        reload();
      }),
    ]);
    return () => {
      unsub.then((fns) => fns.forEach((f) => f()));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const groups: { kind: ModelKind; title: string; hint: string }[] = [
    { kind: "transcription", title: "Transcrição", hint: "Transforma a fala em texto." },
    { kind: "intelligence", title: "Inteligência local", hint: "Gera resumo, decisões, tarefas e prazos." },
  ];

  const choose = async (m: ModelView) => {
    try {
      const s = m.kind === "transcription" ? await api.setTranscriptionModel(m.id) : await api.setIntelligenceModel(m.id);
      onSettings(s);
      reload();
    } catch (e) {
      onError(errorText(e));
    }
  };

  const download = async (id: string) => {
    onError(null);
    try {
      await api.downloadModel(id);
      reload();
    } catch (e) {
      onError(errorText(e));
    }
  };

  const remove = async (id: string) => {
    onError(null);
    try {
      await api.deleteModel(id);
      reload();
    } catch (e) {
      onError(errorText(e));
    }
  };

  return (
    <Section title="Modelos locais">
      <p className="text-sm text-[var(--muted)]">Você pode trocar os modelos a qualquer momento. Modelos maiores são mais precisos, mas usam mais memória.</p>
      {groups.map((g) => (
        <Card key={g.kind} className="flex flex-col gap-4">
          <div>
            <h3 className="font-medium">{g.title}</h3>
            <p className="text-sm text-[var(--muted)]">{g.hint}</p>
          </div>
          <ul className="flex flex-col divide-y divide-black/5 dark:divide-white/5">
            {models
              .filter((m) => m.kind === g.kind)
              .map((m) => {
                const p = progress[m.id];
                const pct = p && p.total ? (p.downloaded / p.total) * 100 : 0;
                return (
                  <li key={m.id} className="flex flex-col gap-2 py-3">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                      <div className="min-w-0">
                        <p className="flex items-center gap-2 font-medium">
                          {m.title}
                          {m.active && <Badge tone="ok">Em uso</Badge>}
                        </p>
                        <p className="text-sm text-[var(--muted)]">{m.purpose}</p>
                        <p className="text-xs text-[var(--muted)]">
                          {formatBytes(m.sizeBytes)} · {m.requirement}
                        </p>
                      </div>
                      <div className="flex flex-wrap gap-2">
                        {m.installed ? (
                          <>
                            {!m.active && (
                              <Button onClick={() => choose(m)}>Usar este</Button>
                            )}
                            <Button variant="ghost" onClick={() => remove(m.id)} disabled={m.active}>
                              Remover
                            </Button>
                          </>
                        ) : m.downloading ? (
                          <Button variant="ghost" onClick={() => api.cancelModelDownload(m.id)}>
                            Pausar
                          </Button>
                        ) : (
                          <Button variant="primary" onClick={() => download(m.id)}>
                            Baixar
                          </Button>
                        )}
                      </div>
                    </div>
                    {m.downloading && p && (
                      <div className="flex flex-col gap-1">
                        <ProgressBar value={pct} label={`Progresso de ${m.title}`} />
                        <p className="text-xs text-[var(--muted)]">
                          {formatBytes(p.downloaded)} de {formatBytes(p.total)}
                          {p.speedBps > 0 && ` · ${formatSpeed(p.speedBps)}`}
                          {p.state === "verificando" && " · conferindo o arquivo…"}
                        </p>
                      </div>
                    )}
                    {!m.installed && !m.downloading && <p className="text-xs text-[var(--muted)]">Não instalado ({formatBytes(m.sizeBytes)})</p>}
                  </li>
                );
              })}
          </ul>
        </Card>
      ))}
    </Section>
  );
}

function AudioSection({ settings, onSettings, onError }: { settings: Settings; onSettings: (s: Settings) => void; onError: (m: string | null) => void }) {
  const [mics, setMics] = useState<AudioDevice[]>([]);
  const [output, setOutput] = useState<string | null>(null);

  useEffect(() => {
    api
      .audioDevices()
      .then((d) => {
        setMics(d.microphones);
        setOutput(d.outputName);
      })
      .catch((e) => onError(errorText(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <Section title="Áudio">
      <Card className="flex flex-col gap-4">
        <label className="flex flex-col gap-2 text-sm">
          <span className="font-medium">Microfone padrão das reuniões</span>
          <select
            className="rounded-lg bg-[var(--bg)] px-3 py-2 ring-1 ring-black/10 dark:ring-white/10"
            value={settings.microphoneId ?? mics.find((m) => m.isDefault)?.id ?? ""}
            onChange={(e) => api.setMicrophone(e.target.value).then(onSettings).catch((err) => onError(errorText(err)))}
          >
            {mics.map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-start gap-3 text-sm">
          <input
            type="checkbox"
            className="mt-1"
            checked={settings.captureSystemAudio}
            onChange={(e) => api.setCaptureSystem(e.target.checked).then(onSettings).catch((err) => onError(errorText(err)))}
          />
          <span>
            <span className="font-medium">Capturar o áudio do computador por padrão</span>
            <span className="block text-[var(--muted)]">
              Registra as outras pessoas da reunião{output ? ` (saída: ${output})` : ""}. Você pode mudar isso em cada reunião.
            </span>
          </span>
        </label>
      </Card>
    </Section>
  );
}

function FoldersSection({
  settings,
  onSettings,
  run,
}: {
  settings: Settings;
  onSettings: (s: Settings) => void;
  run: (fn: () => Promise<unknown>, ok?: string) => Promise<void>;
}) {
  const [paths, setPaths] = useState<StoragePaths | null>(null);
  useEffect(() => {
    api.storagePaths().then(setPaths).catch(() => undefined);
  }, [settings.recordingsDir, settings.modelsDir]);

  const pick = async (kind: "recordings" | "models") => {
    const dir = await open({ directory: true, multiple: false, title: kind === "recordings" ? "Escolha a pasta das gravações" : "Escolha a pasta dos modelos" });
    if (typeof dir !== "string") return;
    await run(async () => {
      const s = kind === "recordings" ? await api.setRecordingsDir(dir) : await api.setModelsDir(dir);
      onSettings(s);
    }, kind === "recordings" ? "Pasta das gravações atualizada. As reuniões antigas continuam onde estão." : "Pasta dos modelos atualizada. Baixe os modelos novamente nela.");
  };

  const reset = (kind: "recordings" | "models") =>
    run(async () => {
      const s = kind === "recordings" ? await api.setRecordingsDir(null) : await api.setModelsDir(null);
      onSettings(s);
    }, "Pasta padrão restaurada.");

  return (
    <Section title="Pastas">
      <Card className="flex flex-col gap-5">
        <FolderRow
          label="Gravações e transcrições"
          path={paths?.recordings}
          custom={settings.recordingsDir !== null}
          onChange={() => pick("recordings")}
          onReset={() => reset("recordings")}
        />
        <FolderRow label="Modelos de IA" path={paths?.models} custom={settings.modelsDir !== null} onChange={() => pick("models")} onReset={() => reset("models")} />
      </Card>
    </Section>
  );
}

function FolderRow({ label, path, custom, onChange, onReset }: { label: string; path?: string; custom: boolean; onChange: () => void; onReset: () => void }) {
  return (
    <div className="flex flex-col gap-2">
      <p className="text-sm font-medium">{label}</p>
      <p className="break-all text-xs text-[var(--muted)]">{path ?? "…"}</p>
      <div className="flex gap-2">
        <Button onClick={onChange}>Escolher pasta…</Button>
        {custom && <Button variant="ghost" onClick={onReset}>Usar pasta padrão</Button>}
        {path && <Button variant="ghost" onClick={() => api.revealPath(path).catch(() => undefined)}>Abrir</Button>}
      </div>
    </div>
  );
}

function DiagnosticsSection({ onError, onNotice }: { onError: (m: string | null) => void; onNotice: (m: string | null) => void }) {
  const [file, setFile] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const exportLogs = async () => {
    setBusy(true);
    onError(null);
    try {
      const path = await api.exportDiagnostics();
      setFile(path);
      onNotice("Diagnóstico salvo. Ele não contém o conteúdo das suas reuniões.");
    } catch (e) {
      onError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Section title="Diagnóstico">
      <Card className="flex flex-col gap-3">
        <p className="text-sm text-[var(--muted)]">
          Se algo não funcionar, gere um arquivo de diagnóstico e envie para o suporte. Ele traz informações do computador e os últimos eventos do aplicativo, sem o conteúdo das reuniões.
        </p>
        <div className="flex flex-wrap items-center gap-3">
          <Button onClick={exportLogs} disabled={busy}>
            {busy ? "Gerando…" : "Exportar diagnóstico"}
          </Button>
          {file && (
            <Button variant="ghost" onClick={() => api.revealPath(file).catch(() => undefined)}>
              Mostrar arquivo
            </Button>
          )}
        </div>
      </Card>
    </Section>
  );
}

function AboutSection() {
  const [info, setInfo] = useState<{ version: string; buildDate: string; dataDir: string } | null>(null);
  useEffect(() => {
    api.appInfo().then(setInfo).catch(() => undefined);
  }, []);
  return (
    <Section title="Sobre">
      <Card className="flex flex-col gap-2 text-sm">
        <p className="font-medium">IntegraMeet {info?.version ?? ""}</p>
        <p className="text-[var(--muted)]">Build {info?.buildDate ?? "—"}</p>
        <p className="text-[var(--muted)]">
          Criado por{" "}
          <span className="font-medium text-[var(--text)]">willian_sdm</span> ·{" "}
          <span className="select-text">https://www.instagram.com/willian_sdm/</span>
        </p>
        <p className="mt-2">Suas reuniões permanecem neste computador. O aplicativo não envia áudio, transcrição ou resumo para nenhum serviço.</p>
      </Card>
    </Section>
  );
}

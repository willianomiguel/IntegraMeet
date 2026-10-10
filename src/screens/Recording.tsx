import { useEffect, useState } from "react";
import { api, AudioDevices, errorText, RecordingStatus, Settings } from "../lib/api";
import { formatClock } from "../lib/format";
import { Badge, Button, Card, LevelMeter, Notice } from "../components/ui";

const SOURCE_LABEL: Record<string, string> = {
  microfone: "Microfone",
  sistema: "Áudio do computador",
};

interface Props {
  settings: Settings;
  onSettings: (s: Settings) => void;
  onStopped: (meetingId: string) => void;
  onBack: () => void;
}

export default function Recording({ settings, onSettings, onStopped, onBack }: Props) {
  const [status, setStatus] = useState<RecordingStatus | null>(null);
  const [checked, setChecked] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Se já existe gravação (usuário saiu da tela e voltou), mostra o painel ao vivo.
  useEffect(() => {
    api
      .recordingStatus()
      .then((s) => setStatus(s))
      .catch(() => undefined)
      .finally(() => setChecked(true));
  }, []);

  useEffect(() => {
    if (!status) return;
    const id = setInterval(() => {
      api
        .recordingStatus()
        .then((s) => {
          if (!s) {
            // A gravação terminou fora desta tela (por exemplo, encerrada pela barra lateral).
            setStatus(null);
            return;
          }
          setStatus(s);
        })
        .catch(() => undefined);
    }, 300);
    return () => clearInterval(id);
  }, [status?.meetingId]);

  if (!checked) return null;

  if (status) {
    return <Live status={status} onStop={async () => {
      setBusy(true);
      setError(null);
      try {
        const id = await api.recordingStop();
        onStopped(id);
      } catch (e) {
        setError(errorText(e));
        setBusy(false);
      }
    }} onPause={async (paused) => {
      try {
        await api.recordingPause(paused);
      } catch (e) {
        setError(errorText(e));
      }
    }} error={error} busy={busy} />;
  }

  return (
    <Setup
      settings={settings}
      onSettings={onSettings}
      onBack={onBack}
      onStarted={(s) => {
        setError(null);
        setStatus(s);
      }}
    />
  );
}

function Setup({
  settings,
  onSettings,
  onBack,
  onStarted,
}: {
  settings: Settings;
  onSettings: (s: Settings) => void;
  onBack: () => void;
  onStarted: (s: RecordingStatus) => void;
}) {
  const [title, setTitle] = useState("");
  const [devices, setDevices] = useState<AudioDevices | null>(null);
  const [mic, setMic] = useState<string | null>(settings.microphoneId);
  const [withSystem, setWithSystem] = useState(settings.captureSystemAudio);
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  useEffect(() => {
    api
      .audioDevices()
      .then((d) => {
        setDevices(d);
        setMic((m) => m ?? d.microphones.find((x) => x.isDefault)?.id ?? d.microphones[0]?.id ?? null);
      })
      .catch((e) => setError(errorText(e)));
  }, []);

  const start = async () => {
    setError(null);
    setStarting(true);
    try {
      if (withSystem !== settings.captureSystemAudio) onSettings(await api.setCaptureSystem(withSystem));
      if (mic !== settings.microphoneId) onSettings(await api.setMicrophone(mic));
      await api.recordingStart(title, mic, withSystem);
      const s = await api.recordingStatus();
      if (s) onStarted(s);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setStarting(false);
    }
  };

  const noMic = devices !== null && devices.microphones.length === 0;

  return (
    <div className="flex flex-col gap-6">
      <header>
        <h1 className="text-2xl font-semibold tracking-tight">Nova reunião</h1>
        <p className="text-sm text-[var(--muted)]">A gravação fica salva neste computador. Nada é enviado para a internet.</p>
      </header>

      {error && <Notice tone="error">{error}</Notice>}
      {noMic && <Notice tone="error">Nenhum microfone encontrado. Conecte um microfone para gravar.</Notice>}

      <Card className="flex flex-col gap-5 p-6">
        <label className="flex flex-col gap-2 text-sm">
          <span className="font-medium">Nome da reunião</span>
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Ex.: Alinhamento semanal de vendas"
            maxLength={120}
            className="rounded-lg bg-[var(--bg)] px-3 py-2 ring-1 ring-black/10 dark:ring-white/10"
          />
        </label>

        <label className="flex flex-col gap-2 text-sm">
          <span className="font-medium">Microfone</span>
          <select
            value={mic ?? ""}
            onChange={(e) => setMic(e.target.value)}
            disabled={!devices || noMic}
            className="rounded-lg bg-[var(--bg)] px-3 py-2 ring-1 ring-black/10 dark:ring-white/10"
          >
            {devices?.microphones.map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
                {m.isDefault ? " (padrão)" : ""}
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-start gap-3 text-sm">
          <input type="checkbox" className="mt-1" checked={withSystem} onChange={(e) => setWithSystem(e.target.checked)} />
          <span>
            <span className="font-medium">Capturar também o áudio do computador</span>
            <span className="block text-[var(--muted)]">Registra as outras pessoas da reunião, pelo som que sai das caixas ou do fone.</span>
          </span>
        </label>

        <div className="flex items-center justify-between pt-2">
          <Button variant="ghost" onClick={onBack}>
            Voltar
          </Button>
          <Button variant="primary" className="px-6 py-3 text-base" onClick={start} disabled={starting || noMic || !mic}>
            {starting ? "Iniciando…" : "Iniciar gravação"}
          </Button>
        </div>
      </Card>
    </div>
  );
}

function Live({
  status,
  onStop,
  onPause,
  error,
  busy,
}: {
  status: RecordingStatus;
  onStop: () => void;
  onPause: (paused: boolean) => void;
  error: string | null;
  busy: boolean;
}) {
  // Só afirmamos "gravando" quando todas as fontes estão de fato ativas.
  const allActive = status.sources.every((s) => s.status.estado === "ativo");
  const failed = status.sources.some((s) => s.status.estado === "falhou");
  const recording = allActive && !status.paused;
  const indicator = status.paused ? "Pausada" : failed ? "Captura com problema" : allActive ? "Gravando" : "Reconectando";
  const tone = status.paused ? "warn" : failed ? "error" : allActive ? "ok" : "warn";

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">{status.title}</h1>
          <p className="text-sm text-[var(--muted)]">Iniciada às {new Date(status.startedAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}</p>
        </div>
        <Badge tone={tone}>
          <span className={`mr-2 inline-block h-2 w-2 rounded-full ${recording ? "animate-pulse bg-red-600" : "bg-current"}`} />
          {indicator}
        </Badge>
      </header>

      {error && <Notice tone="error">{error}</Notice>}
      {status.problem && <Notice tone={failed ? "error" : "warn"}>{status.problem}</Notice>}

      <Card className="flex flex-col items-center gap-6 p-8">
        <p className="font-mono text-6xl font-semibold tabular-nums tracking-tight" aria-live="off">
          {formatClock(status.elapsedMs)}
        </p>

        <div className="grid w-full gap-4 sm:grid-cols-2">
          {status.sources.map((s) => (
            <div key={s.source} className="flex flex-col gap-2 rounded-xl bg-black/[0.03] p-4 dark:bg-white/[0.04]">
              <div className="flex items-center justify-between text-sm">
                <span className="font-medium">{SOURCE_LABEL[s.source] ?? s.source}</span>
                <span className="text-xs text-[var(--muted)]">{labelFor(s.status.estado)}</span>
              </div>
              <p className="truncate text-xs text-[var(--muted)]">{s.deviceName}</p>
              <LevelMeter level={status.paused ? 0 : s.level} />
            </div>
          ))}
        </div>

        <div className="flex flex-wrap justify-center gap-3">
          <Button onClick={() => onPause(!status.paused)} disabled={busy}>
            {status.paused ? "Continuar" : "Pausar"}
          </Button>
          <Button variant="primary" onClick={onStop} disabled={busy}>
            {busy ? "Finalizando…" : "Finalizar reunião"}
          </Button>
        </div>
        <p className="max-w-md text-center text-xs text-[var(--muted)]">
          Ao finalizar, a transcrição começa automaticamente. Você pode continuar usando o computador.
        </p>
      </Card>
    </div>
  );
}

function labelFor(estado: string): string {
  switch (estado) {
    case "ativo":
      return "ativo";
    case "reconectando":
      return "reconectando";
    case "falhou":
      return "falhou";
    default:
      return "iniciando";
  }
}

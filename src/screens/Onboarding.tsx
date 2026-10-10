import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import { api, AudioDevices, errorText, HardwareReport, ModelProgress, ModelView, Profile, Settings, TestReading } from "../lib/api";
import { formatBytes, formatSpeed } from "../lib/format";
import { Badge, Button, Card, LevelMeter, Notice, ProgressBar } from "../components/ui";

const STEPS = ["Boas-vindas", "Computador", "Modelos", "Áudio", "Teste"] as const;

const PROFILE_TEXT: Record<Profile, { name: string; text: string }> = {
  leve: { name: "Leve", text: "Menor consumo de memória e processamento. Ideal para computadores mais simples." },
  balanceado: { name: "Balanceado", text: "Boa combinação entre qualidade e velocidade para a maioria dos computadores." },
  alta: { name: "Alta qualidade", text: "Modelos maiores e mais precisos, para computadores mais fortes." },
};

interface Props {
  settings: Settings;
  onDone: (s: Settings) => void;
}

export default function Onboarding({ settings, onDone }: Props) {
  const [step, setStep] = useState(Math.min(settings.onboardingStep, STEPS.length - 1));
  const [error, setError] = useState<string | null>(null);
  const [current, setCurrent] = useState<Settings>(settings);

  const goTo = (n: number) => {
    setError(null);
    setStep(n);
    api.saveOnboardingStep(n).catch(() => undefined);
  };

  const finish = async () => {
    try {
      const s = await api.completeOnboarding();
      onDone(s);
    } catch (e) {
      setError(errorText(e));
    }
  };

  return (
    <div className="mx-auto flex min-h-screen w-full max-w-3xl flex-col gap-6 px-6 py-10">
      <ol className="flex items-center justify-center gap-2 text-xs text-[var(--muted)]" aria-label="Etapas">
        {STEPS.map((label, i) => (
          <li key={label} aria-current={i === step ? "step" : undefined} className={`flex items-center gap-2 ${i === step ? "font-semibold text-[var(--text)]" : ""}`}>
            <span className={`flex h-5 w-5 items-center justify-center rounded-full text-[10px] ${i <= step ? "bg-[var(--accent)] text-white" : "bg-black/10 dark:bg-white/10"}`}>{i + 1}</span>
            <span className="hidden sm:inline">{label}</span>
          </li>
        ))}
      </ol>

      {error && <Notice tone="error">{error}</Notice>}

      {step === 0 && <Welcome onNext={() => goTo(1)} />}
      {step === 1 && (
        <Computer
          settings={current}
          onProfile={(s) => setCurrent(s)}
          onBack={() => goTo(0)}
          onNext={() => goTo(2)}
          onError={setError}
        />
      )}
      {step === 2 && <Models settings={current} onBack={() => goTo(1)} onNext={() => goTo(3)} onError={setError} />}
      {step === 3 && <AudioSetup settings={current} onSettings={setCurrent} onBack={() => goTo(2)} onNext={() => goTo(4)} onError={setError} />}
      {step === 4 && <FinalTest settings={current} onBack={() => goTo(3)} onFinish={finish} onError={setError} />}
    </div>
  );
}

function Welcome({ onNext }: { onNext: () => void }) {
  return (
    <Card className="flex flex-col gap-6 p-8">
      <div>
        <h1 className="text-3xl font-semibold tracking-tight">Seu assistente privado para reuniões</h1>
        <p className="mt-3 text-[var(--muted)]">Grave, transcreva e organize suas reuniões usando inteligência artificial diretamente no seu computador.</p>
      </div>
      <ul className="grid gap-3 text-sm sm:grid-cols-2">
        <li className="rounded-xl bg-black/[0.03] p-4 dark:bg-white/[0.04]">
          <p className="font-medium">Processamento local</p>
          <p className="mt-1 text-[var(--muted)]">Áudio, transcrição e análise acontecem neste computador.</p>
        </li>
        <li className="rounded-xl bg-black/[0.03] p-4 dark:bg-white/[0.04]">
          <p className="font-medium">Suas reuniões ficam aqui</p>
          <p className="mt-1 text-[var(--muted)]">Nada é enviado para a internet ou para terceiros.</p>
        </li>
        <li className="rounded-xl bg-black/[0.03] p-4 dark:bg-white/[0.04] sm:col-span-2">
          <p className="font-medium">Alguns modelos precisam ser preparados agora</p>
          <p className="mt-1 text-[var(--muted)]">Eles são baixados uma vez. Depois disso, o app funciona sem internet.</p>
        </li>
      </ul>
      <div className="flex justify-end">
        <Button variant="primary" onClick={onNext}>
          Começar configuração
        </Button>
      </div>
    </Card>
  );
}

function Computer({
  settings,
  onProfile,
  onBack,
  onNext,
  onError,
}: {
  settings: Settings;
  onProfile: (s: Settings) => void;
  onBack: () => void;
  onNext: () => void;
  onError: (m: string | null) => void;
}) {
  const [report, setReport] = useState<HardwareReport | null>(null);
  const [chosen, setChosen] = useState<Profile | null>(settings.profile);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .hardware()
      .then((r) => {
        setReport(r);
        setChosen((c) => c ?? settings.profile ?? r.recommended);
      })
      .catch((e) => onError(errorText(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const confirm = async () => {
    if (!chosen) return;
    setBusy(true);
    onError(null);
    try {
      const s = await api.setProfile(chosen);
      onProfile(s);
      onNext();
    } catch (e) {
      onError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (!report) {
    return (
      <Card>
        <p className="text-sm text-[var(--muted)]" role="status">Verificando o computador…</p>
      </Card>
    );
  }

  const { info, recommended } = report;
  const gpu = info.gpus[0];
  const ok = info.windowsSupported && info.diskFreeGb >= 6;

  return (
    <Card className="flex flex-col gap-5 p-7">
      <div>
        <h2 className="text-xl font-semibold">Verificação do computador</h2>
        <p className="mt-1 text-sm text-[var(--muted)]">Analisamos o seu computador para indicar a melhor configuração. Você não precisa escolher nada técnico.</p>
      </div>

      <dl className="grid gap-x-6 gap-y-3 text-sm sm:grid-cols-2">
        <Row label="Sistema" value={info.windowsSupported ? `${info.osCaption} — compatível` : `${info.osCaption} — versão não suportada`} />
        <Row label="Processador" value={`${info.cpuName} (${info.cpuThreads} núcleos lógicos)`} />
        <Row label="Memória" value={`${info.ramGb.toFixed(0)} GB de RAM`} />
        <Row label="Placa de vídeo" value={gpu ? `${gpu.name} — ${(gpu.vramMb / 1024).toFixed(0)} GB de VRAM` : "Não encontrada; o processamento usará o processador"} />
        <Row label="Espaço livre" value={`${info.diskFreeGb.toFixed(0)} GB`} />
        <Row label="Aceleração gráfica" value={info.vulkanAvailable ? "Disponível" : "Indisponível (usará o processador)"} />
      </dl>

      {!info.windowsSupported && <Notice tone="error">Este aplicativo precisa do Windows 10 (versão 2004) ou mais recente.</Notice>}
      {info.windowsSupported && info.diskFreeGb < 6 && (
        <Notice tone="warn">Pouco espaço livre. Os modelos precisam de cerca de 6 GB; libere espaço ou escolha outra pasta nas configurações.</Notice>
      )}

      <fieldset className="flex flex-col gap-2">
        <legend className="mb-2 text-sm font-medium">Desempenho</legend>
        {(Object.keys(PROFILE_TEXT) as Profile[]).map((p) => (
          <label
            key={p}
            className={`flex cursor-pointer items-start gap-3 rounded-xl p-4 ring-1 transition ${chosen === p ? "bg-[var(--accent)]/10 ring-[var(--accent)]" : "ring-black/10 hover:bg-black/[0.02] dark:ring-white/10"}`}
          >
            <input type="radio" name="perfil" className="mt-1" checked={chosen === p} onChange={() => setChosen(p)} />
            <span className="flex-1">
              <span className="flex items-center gap-2 font-medium">
                {PROFILE_TEXT[p].name}
                {p === recommended && <Badge tone="ok">Recomendado</Badge>}
              </span>
              <span className="mt-0.5 block text-sm text-[var(--muted)]">{PROFILE_TEXT[p].text}</span>
            </span>
          </label>
        ))}
      </fieldset>

      <div className="flex justify-between">
        <Button variant="ghost" onClick={onBack}>
          Voltar
        </Button>
        <Button variant="primary" onClick={confirm} disabled={!ok || !chosen || busy}>
          {busy ? "Salvando…" : "Continuar"}
        </Button>
      </div>
    </Card>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-[var(--muted)]">{label}</dt>
      <dd className="font-medium">{value}</dd>
    </div>
  );
}

function Models({ settings, onBack, onNext, onError }: { settings: Settings; onBack: () => void; onNext: () => void; onError: (m: string | null) => void }) {
  const [models, setModels] = useState<ModelView[]>([]);
  const [progress, setProgress] = useState<Record<string, ModelProgress>>({});
  const [failed, setFailed] = useState<Record<string, string>>({});

  const needed = useMemo(
    () => models.filter((m) => m.id === settings.transcriptionModel || m.id === settings.intelligenceModel),
    [models, settings],
  );
  const allReady = needed.length > 0 && needed.every((m) => m.installed);

  const reload = () => api.modelCatalog().then(setModels).catch((e) => onError(errorText(e)));

  useEffect(() => {
    reload();
    const unsubs = Promise.all([
      listen<ModelProgress>("model-progress", (e) => setProgress((p) => ({ ...p, [e.payload.modelId]: e.payload }))),
      listen<string>("model-finished", (e) => {
        setFailed((f) => ({ ...f, [e.payload]: "" }));
        reload();
      }),
      listen<{ id: string; message: string }>("model-failed", (e) => {
        setFailed((f) => ({ ...f, [e.payload.id]: e.payload.message }));
        reload();
      }),
    ]);
    return () => {
      unsubs.then((fns) => fns.forEach((f) => f()));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const start = (id: string) => {
    setFailed((f) => ({ ...f, [id]: "" }));
    api
      .downloadModel(id)
      .then(reload)
      .catch((e) => setFailed((f) => ({ ...f, [id]: errorText(e) })));
  };

  const startMissing = () => needed.filter((m) => !m.installed && !m.downloading).forEach((m) => start(m.id));

  useEffect(() => {
    if (needed.length && needed.some((m) => !m.installed && !m.downloading && !failed[m.id])) {
      // Inicia automaticamente o que falta; o usuário acompanha pelo progresso.
      startMissing();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [needed.length]);

  return (
    <Card className="flex flex-col gap-5 p-7">
      <div>
        <h2 className="text-xl font-semibold">Preparando o computador</h2>
        <p className="mt-1 text-sm text-[var(--muted)]">Estes modelos ficam salvos no seu computador. Depois de baixados, o app funciona sem internet.</p>
      </div>

      {needed.map((m) => {
        const p = progress[m.id];
        const pct = p && p.total > 0 ? (p.downloaded / p.total) * 100 : m.installed ? 100 : 0;
        const err = failed[m.id];
        return (
          <div key={m.id} className="rounded-xl bg-black/[0.03] p-4 dark:bg-white/[0.04]">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="font-medium">{m.kind === "transcription" ? "Preparando transcrição" : "Preparando inteligência local"}</p>
                <p className="text-sm text-[var(--muted)]">{m.purpose}</p>
              </div>
              <Badge tone={m.installed ? "ok" : err ? "error" : "info"}>{m.installed ? "Pronto" : err ? "Falhou" : "Baixando"}</Badge>
            </div>

            {!m.installed && (
              <div className="mt-3 flex flex-col gap-2">
                <ProgressBar value={pct} label={`Progresso de ${m.title}`} />
                <div className="flex flex-wrap justify-between gap-2 text-xs text-[var(--muted)]">
                  <span>
                    {p
                      ? `${formatBytes(p.downloaded)} / ${formatBytes(p.total || m.sizeBytes)}${p.fileCount > 1 ? ` · parte ${p.fileIndex} de ${p.fileCount}` : ""}`
                      : `Tamanho aproximado: ${formatBytes(m.sizeBytes)}`}
                  </span>
                  <span>
                    {p?.state === "verificando" ? "Conferindo o arquivo…" : p && p.speedBps > 0 ? formatSpeed(p.speedBps) : m.downloading ? "Iniciando…" : ""}
                  </span>
                </div>
                <p className="text-xs text-[var(--muted)]">Requisito: {m.requirement}</p>
              </div>
            )}

            {err && (
              <div className="mt-3 flex items-center justify-between gap-3">
                <p className="text-sm text-red-700 dark:text-red-300">{err}</p>
                <Button onClick={() => start(m.id)}>Tentar novamente</Button>
              </div>
            )}
          </div>
        );
      })}

      {needed.length === 0 && <Notice tone="warn">Nenhum modelo foi escolhido. Volte e confirme o desempenho.</Notice>}

      <div className="flex justify-between">
        <Button variant="ghost" onClick={onBack}>
          Voltar
        </Button>
        <Button variant="primary" disabled={!allReady} onClick={onNext}>
          {allReady ? "Continuar" : "Aguardando downloads…"}
        </Button>
      </div>
    </Card>
  );
}

function useLevelPolling(active: boolean, onReading?: (r: TestReading | null) => void) {
  const [reading, setReading] = useState<TestReading | null>(null);
  useEffect(() => {
    if (!active) {
      setReading(null);
      return;
    }
    const id = setInterval(() => {
      api
        .audioTestRead()
        .then((r) => {
          setReading(r);
          onReading?.(r);
        })
        .catch(() => undefined);
    }, 150);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);
  return reading;
}

function AudioSetup({
  settings,
  onSettings,
  onBack,
  onNext,
  onError,
}: {
  settings: Settings;
  onSettings: (s: Settings) => void;
  onBack: () => void;
  onNext: () => void;
  onError: (m: string | null) => void;
}) {
  const [devices, setDevices] = useState<AudioDevices | null>(null);
  const [mic, setMic] = useState<string | null>(settings.microphoneId);
  const [sysOn, setSysOn] = useState(settings.captureSystemAudio);
  const [testing, setTesting] = useState<"microfone" | "sistema" | null>(null);
  const [sample, setSample] = useState<{ path: string; source: string } | null>(null);
  const reading = useLevelPolling(testing !== null);
  const peakRef = useRef(0);

  useEffect(() => {
    api
      .audioDevices()
      .then((d) => {
        setDevices(d);
        setMic((m) => m ?? d.microphones.find((x) => x.isDefault)?.id ?? d.microphones[0]?.id ?? null);
      })
      .catch((e) => onError(errorText(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (reading) peakRef.current = Math.max(peakRef.current, reading.level);
  }, [reading]);

  const saveMic = (id: string) => {
    setMic(id);
    api.setMicrophone(id).then(onSettings).catch((e) => onError(errorText(e)));
  };

  const toggleSystem = (on: boolean) => {
    setSysOn(on);
    api.setCaptureSystem(on).then(onSettings).catch((e) => onError(errorText(e)));
  };

  const startTest = async (source: "microfone" | "sistema") => {
    onError(null);
    setSample(null);
    peakRef.current = 0;
    try {
      await api.audioTestStart(source, source === "microfone" ? mic : null);
      setTesting(source);
    } catch (e) {
      onError(errorText(e));
    }
  };

  const stopTest = async () => {
    if (!testing) return;
    const source = testing;
    setTesting(null);
    try {
      const r = await api.audioTestStop(source);
      if (r.peak < 0.01 && r.durationMs > 1000) {
        onError(
          source === "microfone"
            ? "Não ouvimos nenhum som do microfone. Verifique se ele está ligado, selecionado e sem o mudo ativado no Windows."
            : "Não ouvimos som do computador. Toque um áudio durante o teste, como um vídeo, e tente de novo.",
        );
      }
      setSample({ path: r.path, source });
    } catch (e) {
      onError(errorText(e));
    }
  };

  const level = reading?.level ?? 0;
  const micOk = devices && devices.microphones.length > 0;

  return (
    <Card className="flex flex-col gap-6 p-7">
      <div>
        <h2 className="text-xl font-semibold">Áudio das reuniões</h2>
        <p className="mt-1 text-sm text-[var(--muted)]">Escolha de onde o app vai ouvir. Vamos testar antes da sua primeira reunião.</p>
      </div>

      <section className="flex flex-col gap-3">
        <h3 className="font-medium">Microfone</h3>
        {!devices && <p className="text-sm text-[var(--muted)]">Procurando microfones…</p>}
        {devices && !micOk && <Notice tone="error">Nenhum microfone encontrado. Conecte um microfone e reabra esta etapa.</Notice>}
        {micOk && (
          <>
            <select
              aria-label="Microfone"
              className="rounded-lg bg-[var(--surface)] px-3 py-2 text-sm ring-1 ring-black/10 dark:ring-white/10"
              value={mic ?? ""}
              onChange={(e) => saveMic(e.target.value)}
              disabled={testing !== null}
            >
              {devices?.microphones.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                  {m.isDefault ? " (padrão)" : ""}
                </option>
              ))}
            </select>
            <div className="flex items-center gap-4">
              <LevelMeter level={testing === "microfone" ? level : 0} />
              <div className="flex gap-2">
                {testing === "microfone" ? (
                  <Button variant="danger" onClick={stopTest}>
                    Parar teste
                  </Button>
                ) : (
                  <Button onClick={() => startTest("microfone")} disabled={testing !== null}>
                    Testar microfone
                  </Button>
                )}
              </div>
            </div>
            {testing === "microfone" && <p className="text-sm text-[var(--muted)]" role="status">Fale normalmente. A barra deve se mover.</p>}
            {sample?.source === "microfone" && <audio controls className="w-full" src={convertFileSrc(sample.path)} />}
          </>
        )}
      </section>

      <section className="flex flex-col gap-3 border-t border-black/5 pt-5 dark:border-white/5">
        <h3 className="font-medium">Áudio do computador</h3>
        <p className="text-sm text-[var(--muted)]">
          Permite que o app também escute as outras pessoas da reunião, captando o som que sai das caixas de som ou do fone
          {devices?.outputName ? ` (${devices.outputName})` : ""}.
        </p>
        <label className="flex items-center gap-3 text-sm">
          <input type="checkbox" checked={sysOn} onChange={(e) => toggleSystem(e.target.checked)} disabled={testing !== null} />
          Capturar também o áudio do computador nas reuniões
        </label>
        {sysOn && (
          <div className="flex flex-wrap items-center gap-4">
            <LevelMeter level={testing === "sistema" ? level : 0} />
            {testing === "sistema" ? (
              <Button variant="danger" onClick={stopTest}>
                Parar teste
              </Button>
            ) : (
              <Button onClick={() => startTest("sistema")} disabled={testing !== null}>
                Testar áudio do computador
              </Button>
            )}
            {testing === "sistema" && <p className="text-sm text-[var(--muted)]" role="status">Toque um vídeo ou música agora.</p>}
          </div>
        )}
        {sample?.source === "sistema" && <audio controls className="w-full" src={convertFileSrc(sample.path)} />}
      </section>

      <div className="flex justify-between">
        <Button variant="ghost" onClick={onBack} disabled={testing !== null}>
          Voltar
        </Button>
        <Button variant="primary" onClick={onNext} disabled={testing !== null}>
          Continuar
        </Button>
      </div>
    </Card>
  );
}

function FinalTest({
  settings,
  onBack,
  onFinish,
  onError,
}: {
  settings: Settings;
  onBack: () => void;
  onFinish: () => void;
  onError: (m: string | null) => void;
}) {
  const [phase, setPhase] = useState<"pronto" | "gravando" | "transcrevendo" | "ok" | "falhou">("pronto");
  const [seconds, setSeconds] = useState(0);
  const [wav, setWav] = useState<string | null>(null);
  const [text, setText] = useState<string>("");
  const reading = useLevelPolling(phase === "gravando");

  useEffect(() => {
    if (phase !== "gravando") return;
    const id = setInterval(() => {
      setSeconds((s) => s + 1);
    }, 1000);
    return () => clearInterval(id);
  }, [phase]);

  useEffect(() => {
    if (phase === "gravando" && seconds >= 5) {
      void stopAndTranscribe();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seconds, phase]);

  const record = async () => {
    onError(null);
    setText("");
    setWav(null);
    setSeconds(0);
    try {
      await api.audioTestStart("microfone", settings.microphoneId);
      setPhase("gravando");
    } catch (e) {
      setPhase("falhou");
      onError(errorText(e));
    }
  };

  const stopAndTranscribe = async () => {
    setPhase("transcrevendo");
    try {
      const r = await api.audioTestStop("microfone");
      setWav(r.path);
      if (r.peak < 0.01) {
        setPhase("falhou");
        onError("O microfone não captou som no teste. Confira o microfone escolhido e tente novamente.");
        return;
      }
      const transcript = await api.audioTestTranscribe(r.path);
      setText(transcript);
      setPhase("ok");
    } catch (e) {
      setPhase("falhou");
      onError(errorText(e));
    }
  };

  const level = phase === "gravando" ? reading?.level ?? 0 : 0;

  return (
    <Card className="flex flex-col gap-6 p-7">
      <div>
        <h2 className="text-xl font-semibold">Teste rápido</h2>
        <p className="mt-1 text-sm text-[var(--muted)]">Vamos gravar 5 segundos, tocar de volta e transcrever. Assim você sabe que tudo está funcionando.</p>
      </div>

      <div className="flex flex-col items-center gap-4 rounded-2xl bg-black/[0.03] p-6 dark:bg-white/[0.04]">
        <LevelMeter level={level} bars={32} />
        {phase === "pronto" && (
          <Button variant="primary" onClick={record}>
            Gravar 5 segundos
          </Button>
        )}
        {phase === "gravando" && <p className="text-sm" role="status">Gravando… {Math.max(0, 5 - seconds)} s. Fale algo.</p>}
        {phase === "transcrevendo" && <p className="text-sm" role="status">Transcrevendo o trecho de teste…</p>}
        {phase === "falhou" && (
          <Button onClick={record}>Tentar novamente</Button>
        )}
      </div>

      {wav && <audio controls className="w-full" src={convertFileSrc(wav)} />}
      {phase === "ok" && (
        <Notice tone="ok">
          <p className="font-medium">Tudo funcionando.</p>
          <p className="mt-1">Transcrição do teste: {text ? `“${text}”` : "(nenhuma fala reconhecida; tente falar mais perto do microfone)"}</p>
        </Notice>
      )}

      <div className="flex justify-between">
        <Button variant="ghost" onClick={onBack} disabled={phase === "gravando" || phase === "transcrevendo"}>
          Voltar
        </Button>
        <Button variant="primary" onClick={onFinish} disabled={phase !== "ok"}>
          Iniciar minha primeira reunião
        </Button>
      </div>

      {phase === "ok" && <p className="text-center text-sm text-[var(--muted)]">Tudo pronto! Seu computador está preparado.</p>}
    </Card>
  );
}

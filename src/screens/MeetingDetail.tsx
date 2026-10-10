import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { api, AnalysisItem, errorText, ItemKind, MeetingDetail as Detail, Segment } from "../lib/api";
import { formatClock, formatDateTime, ITEM_LABEL, STATUS_LABEL } from "../lib/format";
import { isNoiseText } from "../lib/noise";
import { Badge, Button, Card, EmptyState, Notice } from "../components/ui";

type Tab = "visao" | "transcricao" | "tarefas" | "momentos";

const SOURCE_NAME: Record<string, string> = {
  microfone: "Microfone",
  sistema: "Áudio do computador",
};

const PROCESSING = new Set(["transcrevendo", "analisando"]);

interface Props {
  id: string;
  initialSeekMs?: number;
  onBack: () => void;
  onDeleted: () => void;
}

export default function MeetingDetail({ id, initialSeekMs, onBack, onDeleted }: Props) {
  const [detail, setDetail] = useState<Detail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("visao");
  const [currentMs, setCurrentMs] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [title, setTitle] = useState("");
  const audios = useRef<Record<string, HTMLAudioElement | null>>({});

  const load = () =>
    api
      .meetingDetail(id)
      .then((d) => {
        setDetail(d);
        setTitle((t) => t || d.title);
      })
      .catch((e) => setError(errorText(e)));

  useEffect(() => {
    load();
    // Atualiza enquanto a reunião estiver sendo processada.
    const timer = setInterval(() => {
      api
        .meetingDetail(id)
        .then((d) => {
          setDetail(d);
          if (!PROCESSING.has(d.status)) clearInterval(timer);
        })
        .catch(() => undefined);
    }, 2000);
    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  const primaryAudio = () => Object.values(audios.current).find((a) => a) ?? null;

  const seek = (ms: number, autoplay = true) => {
    for (const el of Object.values(audios.current)) {
      if (!el) continue;
      el.currentTime = Math.max(0, ms / 1000);
      if (autoplay) el.play().catch(() => undefined);
    }
    setCurrentMs(ms);
  };

  // Abre a reunião já no trecho pedido pela pesquisa.
  useEffect(() => {
    if (initialSeekMs === undefined || !detail) return;
    const t = setTimeout(() => seek(initialSeekMs, false), 300);
    setTab("transcricao");
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detail?.id]);

  const togglePlay = () => {
    const el = primaryAudio();
    if (!el) return;
    if (el.paused) Object.values(audios.current).forEach((a) => a?.play().catch(() => undefined));
    else Object.values(audios.current).forEach((a) => a?.pause());
  };

  const act = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
      await load();
    } catch (e) {
      setError(errorText(e));
    }
  };

  const rename = async () => {
    if (!detail) return;
    const next = title.trim();
    if (!next || next === detail.title) {
      setTitle(detail.title);
      return;
    }
    await act(() => api.meetingRename(id, next));
  };

  const remove = async () => {
    try {
      await api.meetingDelete(id);
      onDeleted();
    } catch (e) {
      setError(errorText(e));
      setConfirmDelete(false);
    }
  };

  const activeSegment = useMemo(
    () => detail?.segments.find((s) => currentMs >= s.startMs && currentMs < s.endMs) ?? null,
    [detail, currentMs],
  );

  if (error && !detail) {
    return (
      <div className="flex flex-col gap-4">
        <Button variant="ghost" onClick={onBack} className="self-start">
          ← Voltar
        </Button>
        <Notice tone="error">{error}</Notice>
      </div>
    );
  }
  if (!detail) return <p role="status" className="text-sm text-[var(--muted)]">Carregando reunião…</p>;

  const processing = PROCESSING.has(detail.status);
  const hasSegments = detail.segments.length > 0;
  const hasAudio = detail.recordings.length > 0;

  return (
    <div className="flex flex-col gap-6">
      <div>
        <Button variant="ghost" onClick={onBack} className="-ml-3">
          ← Início
        </Button>
      </div>

      <header className="flex flex-col gap-3">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0 flex-1">
            <input
              aria-label="Título da reunião"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              onBlur={rename}
              onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
              className="w-full bg-transparent text-2xl font-semibold tracking-tight outline-none focus-visible:rounded focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
            />
            <p className="text-sm text-[var(--muted)]">
              {formatDateTime(detail.startedAt)}
              {detail.durationMs > 0 && ` · ${formatClock(detail.durationMs)}`}
            </p>
          </div>
          <Badge tone={detail.status === "erro" || detail.status === "gravacao_interrompida" ? "warn" : detail.status === "analisada" ? "ok" : "neutral"}>
            {STATUS_LABEL[detail.status] ?? detail.status}
          </Badge>
        </div>

        <div className="flex flex-wrap gap-2">
          {processing ? (
            <Button onClick={() => act(() => api.meetingCancel(id))}>Cancelar processamento</Button>
          ) : (
            <>
              {detail.status !== "gravando" && (
                <Button onClick={() => act(() => api.meetingTranscribe(id))}>
                  {hasSegments ? "Transcrever de novo" : "Transcrever"}
                </Button>
              )}
              {hasSegments && (
                <Button onClick={() => act(() => api.meetingAnalyze(id))}>
                  {detail.analysis ? "Analisar de novo" : "Analisar com IA"}
                </Button>
              )}
            </>
          )}
          <span className="flex-1" />
          {!confirmDelete ? (
            <Button variant="ghost" onClick={() => setConfirmDelete(true)} disabled={detail.status === "gravando"}>
              Excluir reunião
            </Button>
          ) : (
            <span className="flex items-center gap-2 text-sm">
              <span>Apagar a reunião e os áudios?</span>
              <Button variant="danger" onClick={remove}>
                Sim, excluir
              </Button>
              <Button variant="ghost" onClick={() => setConfirmDelete(false)}>
                Cancelar
              </Button>
            </span>
          )}
        </div>
      </header>

      {error && <Notice tone="error">{error}</Notice>}
      {detail.error && <Notice tone={detail.status === "erro" ? "warn" : "info"}>{detail.error}</Notice>}
      {processing && <Notice tone="info">{detail.status === "transcrevendo" ? "Transcrevendo a reunião. Isso pode levar alguns minutos." : "Analisando a transcrição com a inteligência local."}</Notice>}

      {hasAudio && (
        <Card className="flex items-center gap-4 p-4">
          <Button onClick={togglePlay} aria-label={playing ? "Pausar" : "Reproduzir"} className="h-10 w-10 shrink-0 !p-0">
            {playing ? "❚❚" : "▶"}
          </Button>
          <input
            type="range"
            aria-label="Posição do áudio"
            min={0}
            max={Math.max(detail.durationMs, 1)}
            value={Math.min(currentMs, Math.max(detail.durationMs, 1))}
            onChange={(e) => seek(Number(e.target.value), false)}
            className="flex-1 accent-[var(--accent)]"
          />
          <span className="w-28 text-right font-mono text-sm tabular-nums">
            {formatClock(currentMs)} / {formatClock(detail.durationMs)}
          </span>
          {detail.recordings.map((r, i) => (
            <audio
              key={r.source}
              ref={(el) => {
                audios.current[r.source] = el;
              }}
              src={convertFileSrc(r.filePath)}
              preload="metadata"
              onPlay={() => i === 0 && setPlaying(true)}
              onPause={() => i === 0 && setPlaying(false)}
              onEnded={() => i === 0 && setPlaying(false)}
              onTimeUpdate={(e) => i === 0 && setCurrentMs(e.currentTarget.currentTime * 1000)}
            />
          ))}
        </Card>
      )}

      <nav className="flex gap-1 border-b border-black/5 dark:border-white/5" aria-label="Seções da reunião">
        {(
          [
            ["visao", "Visão geral"],
            ["transcricao", "Transcrição"],
            ["tarefas", "Tarefas e decisões"],
            ["momentos", "Momentos"],
          ] as [Tab, string][]
        ).map(([key, label]) => (
          <button
            key={key}
            onClick={() => setTab(key)}
            aria-current={tab === key ? "page" : undefined}
            className={`-mb-px border-b-2 px-4 py-2 text-sm transition ${tab === key ? "border-[var(--accent)] font-medium text-[var(--text)]" : "border-transparent text-[var(--muted)] hover:text-[var(--text)]"}`}
          >
            {label}
          </button>
        ))}
      </nav>

      {tab === "visao" && <Overview detail={detail} onAnalyze={() => act(() => api.meetingAnalyze(id))} />}
      {tab === "transcricao" &&
        (hasSegments ? (
          <Transcript segments={detail.segments} activeId={activeSegment?.id ?? null} onSeek={seek} />
        ) : (
          <EmptyState title="Sem transcrição ainda" text={processing ? "Aguarde: estamos transcrevendo a reunião." : "Clique em Transcrever para gerar o texto desta reunião."} />
        ))}
      {tab === "tarefas" && <Items detail={detail} onSeek={seek} />}
      {tab === "momentos" && <Moments detail={detail} onSeek={seek} />}
    </div>
  );
}

function Overview({ detail, onAnalyze }: { detail: Detail; onAnalyze: () => void }) {
  const a = detail.analysis;
  if (!a) {
    return (
      <EmptyState
        title="Análise ainda não feita"
        text={
          detail.segments.length
            ? "Gere o resumo, as decisões e as tarefas a partir da transcrição. Tudo acontece neste computador."
            : "A análise precisa da transcrição. Transcreva a reunião primeiro."
        }
        action={detail.segments.length ? <Button variant="primary" onClick={onAnalyze}>Analisar com IA</Button> : undefined}
      />
    );
  }
  const count = (k: ItemKind) => a.items.filter((i) => i.kind === k).length;
  return (
    <div className="flex flex-col gap-5">
      <Card>
        <h2 className="mb-2 font-medium">Resumo</h2>
        <p className="leading-relaxed">{a.summary || "O modelo não gerou um resumo para esta reunião."}</p>
        {a.topics.length > 0 && (
          <div className="mt-4 flex flex-wrap gap-2">
            {a.topics.map((t) => (
              <Badge key={t}>{t}</Badge>
            ))}
          </div>
        )}
      </Card>
      <div className="grid gap-3 sm:grid-cols-4">
        {(["decisao", "tarefa", "pendencia", "risco"] as ItemKind[]).map((k) => (
          <Card key={k} className="p-4">
            <p className="text-sm text-[var(--muted)]">{ITEM_LABEL[k]}s</p>
            <p className="text-2xl font-semibold">{count(k)}</p>
          </Card>
        ))}
      </div>
      <p className="text-xs text-[var(--muted)]">
        Análise feita por {a.model} em {formatDateTime(a.createdAt)}. Itens só aparecem quando citam um trecho da transcrição.
      </p>
      {detail.segments.length > 0 && a.items.length === 0 && <Notice tone="info">Nenhuma decisão, tarefa ou risco foi identificado com segurança nesta reunião.</Notice>}
    </div>
  );
}

const HIDE_NOISE_KEY = "integrameet.ocultarSons";

function readHideNoise(): boolean {
  try {
    return localStorage.getItem(HIDE_NOISE_KEY) === "1";
  } catch {
    return false;
  }
}

function Transcript({ segments, activeId, onSeek }: { segments: Segment[]; activeId: number | null; onSeek: (ms: number) => void }) {
  const activeRef = useRef<HTMLLIElement | null>(null);
  const [hideNoise, setHideNoise] = useState(readHideNoise);
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [activeId]);

  const noiseCount = useMemo(() => segments.filter((s) => isNoiseText(s.text)).length, [segments]);
  const visible = useMemo(() => (hideNoise ? segments.filter((s) => !isNoiseText(s.text)) : segments), [segments, hideNoise]);

  const toggleNoise = (on: boolean) => {
    setHideNoise(on);
    try {
      localStorage.setItem(HIDE_NOISE_KEY, on ? "1" : "0");
    } catch {
      // Sem armazenamento local, a preferência vale só nesta sessão.
    }
  };

  const copy = async () => {
    const text = visible
      .map((s) => `[${formatClock(s.startMs)}] ${SOURCE_NAME[s.source] ?? s.source}: ${s.text}`)
      .join("\n");
    try {
      await navigator.clipboard.writeText(text);
      setCopied(`${visible.length} trechos copiados`);
    } catch {
      // Alguns ambientes bloqueiam a API do clipboard: usamos a cópia clássica.
      const area = document.createElement("textarea");
      area.value = text;
      area.setAttribute("readonly", "");
      area.style.position = "fixed";
      area.style.opacity = "0";
      document.body.appendChild(area);
      area.select();
      const ok = document.execCommand("copy");
      document.body.removeChild(area);
      setCopied(ok ? `${visible.length} trechos copiados` : "Não foi possível copiar. Selecione o texto e use Ctrl+C.");
    }
    setTimeout(() => setCopied(null), 2500);
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-col gap-1">
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={hideNoise} onChange={(e) => toggleNoise(e.target.checked)} />
            Ocultar sons de ambiente (barulho, música, risos)
          </label>
          {noiseCount > 0 && (
            <p className="text-xs text-[var(--muted)]">
              {hideNoise ? `${noiseCount} trecho(s) de som oculto(s).` : `${noiseCount} trecho(s) de som encontrado(s).`}
            </p>
          )}
        </div>
        <div className="flex items-center gap-3">
          {copied && (
            <span role="status" className="text-xs text-[var(--muted)]">
              {copied}
            </span>
          )}
          <Button onClick={copy}>Copiar transcrição</Button>
        </div>
      </div>

      <p className="text-xs text-[var(--muted)]">Clique em um trecho para ouvir a partir dele.</p>
      <ol className="flex flex-col gap-1">
        {visible.map((s) => (
          <li key={s.id} ref={s.id === activeId ? activeRef : undefined}>
            <button
              onClick={() => onSeek(s.startMs)}
              className={`flex w-full gap-4 rounded-lg px-3 py-2 text-left text-sm transition hover:bg-black/[0.04] dark:hover:bg-white/[0.05] ${s.id === activeId ? "bg-[var(--accent)]/10 ring-1 ring-[var(--accent)]/40" : ""} ${isNoiseText(s.text) ? "italic text-[var(--muted)]" : ""}`}
            >
              <span className="w-14 shrink-0 font-mono text-xs text-[var(--muted)] tabular-nums">{formatClock(s.startMs)}</span>
              <span className="w-32 shrink-0 truncate text-xs text-[var(--muted)]">{SOURCE_NAME[s.source] ?? s.source}</span>
              <span className="flex-1 leading-relaxed">{s.text}</span>
            </button>
          </li>
        ))}
      </ol>
    </div>
  );
}

function Items({ detail, onSeek }: { detail: Detail; onSeek: (ms: number) => void }) {
  const a = detail.analysis;
  if (!a) return <EmptyState title="Nada para mostrar ainda" text="Depois da análise, as decisões e tarefas aparecem aqui, cada uma com o trecho de onde veio." />;
  const kinds: ItemKind[] = ["decisao", "tarefa", "pendencia", "pergunta", "risco"];
  const groups = kinds.map((k) => ({ kind: k, items: a.items.filter((i) => i.kind === k) })).filter((g) => g.items.length);
  if (groups.length === 0) return <EmptyState title="Nenhum item identificado" text="A análise não encontrou decisões ou tarefas com base no que foi dito." />;

  return (
    <div className="flex flex-col gap-6">
      <p className="text-xs text-[var(--muted)]">Responsável e prazo aparecem apenas quando foram citados na reunião.</p>
      {groups.map((g) => (
        <section key={g.kind} className="flex flex-col gap-2">
          <h2 className="font-medium">{ITEM_LABEL[g.kind]}s</h2>
          <ul className="flex flex-col gap-2">
            {g.items.map((item) => (
              <ItemRow key={item.id} item={item} detail={detail} onSeek={onSeek} />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

function Moments({ detail, onSeek }: { detail: Detail; onSeek: (ms: number) => void }) {
  const moments = detail.analysis?.items.filter((i) => i.kind === "momento") ?? [];
  if (moments.length === 0) return <EmptyState title="Nenhum momento destacado" text="Trechos importantes que não são decisões ou tarefas aparecem aqui." />;
  return (
    <ul className="flex flex-col gap-2">
      {moments.map((item) => (
        <ItemRow key={item.id} item={item} detail={detail} onSeek={onSeek} />
      ))}
    </ul>
  );
}

function ItemRow({ item, detail, onSeek }: { item: AnalysisItem; detail: Detail; onSeek: (ms: number) => void }) {
  const first = detail.segments.find((s) => item.segmentIds.includes(s.id));
  return (
    <li className="flex flex-col gap-2 rounded-xl bg-[var(--surface)] p-4 ring-1 ring-black/5 dark:ring-white/5">
      <p className="leading-relaxed">{item.text}</p>
      <div className="flex flex-wrap items-center gap-2 text-xs">
        {item.owner && <Badge tone="info">Responsável: {item.owner}</Badge>}
        {item.due && <Badge tone="warn">Prazo: {item.due}</Badge>}
        {first && (
          <button onClick={() => onSeek(first.startMs)} className="ml-auto text-[var(--accent)] underline-offset-2 hover:underline">
            Ouvir trecho ({formatClock(first.startMs)})
          </button>
        )}
      </div>
    </li>
  );
}

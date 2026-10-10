import { useEffect, useState } from "react";
import { api, errorText, MeetingSummary, ProcessingItem, SearchHit } from "../lib/api";
import { formatClock, formatWhen, STATUS_LABEL } from "../lib/format";
import { Badge, Button, Card, EmptyState, Notice, ProgressBar } from "../components/ui";

const KIND_LABEL: Record<string, string> = {
  transcrever: "Transcrevendo",
  analisar: "Analisando",
};

interface Props {
  onNewMeeting: () => void;
  onOpenMeeting: (id: string, seekMs?: number) => void;
}

export default function Home({ onNewMeeting, onOpenMeeting }: Props) {
  const [meetings, setMeetings] = useState<MeetingSummary[] | null>(null);
  const [processing, setProcessing] = useState<ProcessingItem[]>([]);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadMeetings = () =>
    api
      .meetingsList()
      .then(setMeetings)
      .catch((e) => setError(errorText(e)));

  useEffect(() => {
    loadMeetings();
  }, []);

  // Processamento em andamento: atualiza com frequência enquanto há algo rodando.
  useEffect(() => {
    let alive = true;
    const tick = () =>
      api
        .processingStatus()
        .then((items) => {
          if (!alive) return;
          setProcessing(items);
          if (items.length === 0) loadMeetings();
        })
        .catch(() => undefined);
    tick();
    const id = setInterval(tick, 2000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  // Busca com pequeno atraso para não consultar a cada tecla.
  useEffect(() => {
    const q = query.trim();
    if (q.length < 2) {
      setHits(null);
      return;
    }
    const id = setTimeout(() => {
      api
        .search(q)
        .then(setHits)
        .catch((e) => setError(errorText(e)));
    }, 250);
    return () => clearTimeout(id);
  }, [query]);

  return (
    <div className="flex flex-col gap-8">
      <header className="flex flex-col gap-5 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Início</h1>
          <p className="text-sm text-[var(--muted)]">Suas reuniões ficam guardadas neste computador.</p>
        </div>
        <Button variant="primary" className="px-6 py-3 text-base" onClick={onNewMeeting}>
          Nova reunião
        </Button>
      </header>

      {error && <Notice tone="error">{error}</Notice>}

      <label className="flex flex-col gap-2">
        <span className="sr-only">Pesquisar reuniões</span>
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Pesquisar em títulos e no que foi dito nas reuniões"
          className="rounded-xl bg-[var(--surface)] px-4 py-3 text-sm ring-1 ring-black/10 focus:outline-2 focus:outline-[var(--accent)] dark:ring-white/10"
        />
      </label>

      {hits && (
        <Card>
          <h2 className="mb-3 font-medium">Resultados</h2>
          {hits.length === 0 ? (
            <p className="text-sm text-[var(--muted)]">Nada encontrado para “{query.trim()}”.</p>
          ) : (
            <ul className="flex flex-col divide-y divide-black/5 dark:divide-white/5">
              {hits.map((h, i) => (
                <li key={`${h.meetingId}-${h.segmentId ?? i}`}>
                  <button
                    className="flex w-full flex-col gap-1 py-3 text-left hover:opacity-80"
                    onClick={() => onOpenMeeting(h.meetingId, h.startMs ?? undefined)}
                  >
                    <span className="flex items-center gap-2 text-sm font-medium">
                      {h.meetingTitle}
                      {h.startMs !== null && <span className="text-xs font-normal text-[var(--muted)]">em {formatClock(h.startMs)}</span>}
                    </span>
                    {h.snippet && <span className="text-sm text-[var(--muted)]">{renderSnippet(h.snippet)}</span>}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </Card>
      )}

      {processing.length > 0 && (
        <Card>
          <h2 className="mb-3 font-medium">Em andamento</h2>
          <ul className="flex flex-col gap-4">
            {processing.map((p, i) => (
              <li key={`${p.meetingId}-${p.kind}-${i}`} className="flex flex-col gap-2">
                <div className="flex items-center justify-between gap-3 text-sm">
                  <span className="truncate">
                    <span className="font-medium">{p.title}</span>
                    <span className="text-[var(--muted)]"> · {KIND_LABEL[p.kind] ?? p.kind}</span>
                  </span>
                  <span className="flex items-center gap-3">
                    {p.status === "pendente" ? <Badge>Na fila</Badge> : <span className="text-xs text-[var(--muted)]">{p.progress ?? 0}%</span>}
                    <button className="text-xs text-[var(--muted)] underline" onClick={() => api.meetingCancel(p.meetingId).then(() => undefined)}>
                      Cancelar
                    </button>
                  </span>
                </div>
                {p.status === "executando" && <ProgressBar value={p.progress ?? 0} label={`Progresso de ${p.title}`} />}
              </li>
            ))}
          </ul>
        </Card>
      )}

      <section className="flex flex-col gap-3">
        <h2 className="font-medium">Reuniões recentes</h2>
        {meetings === null && <p className="text-sm text-[var(--muted)]">Carregando…</p>}
        {meetings && meetings.length === 0 && (
          <EmptyState
            title="Nenhuma reunião ainda"
            text="Quando você gravar uma reunião, ela aparece aqui com a transcrição e o resumo."
            action={
              <Button variant="primary" onClick={onNewMeeting}>
                Começar primeira reunião
              </Button>
            }
          />
        )}
        {meetings && meetings.length > 0 && (
          <ul className="flex flex-col gap-2">
            {meetings.map((m) => (
              <li key={m.id}>
                <button
                  onClick={() => onOpenMeeting(m.id)}
                  className="flex w-full items-center justify-between gap-4 rounded-xl bg-[var(--surface)] px-5 py-4 text-left ring-1 ring-black/5 transition hover:ring-[var(--accent)] dark:ring-white/5"
                >
                  <span className="min-w-0">
                    <span className="block truncate font-medium">{m.title}</span>
                    <span className="text-xs text-[var(--muted)]">
                      {formatWhen(m.startedAt)} · {m.durationMs > 0 ? formatClock(m.durationMs) : "duração não registrada"}
                    </span>
                  </span>
                  <Badge tone={m.status === "analisada" || m.status === "transcrita" ? "ok" : m.status === "erro" || m.status === "gravacao_interrompida" ? "warn" : "neutral"}>
                    {STATUS_LABEL[m.status] ?? m.status}
                  </Badge>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

/** O núcleo marca os termos encontrados com << >>. Mostramos em negrito sem usar HTML. */
function renderSnippet(snippet: string) {
  const parts = snippet.split(/(<<.*?>>)/g);
  return parts.map((part, i) =>
    part.startsWith("<<") && part.endsWith(">>") ? (
      <mark key={i} className="rounded bg-amber-300/60 px-0.5 text-[var(--text)] dark:bg-amber-400/40">
        {part.slice(2, -2)}
      </mark>
    ) : (
      <span key={i}>{part}</span>
    ),
  );
}

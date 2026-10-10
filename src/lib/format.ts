export function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1).replace(".", ",")} GB`;
  if (bytes >= 1024 ** 2) return `${Math.round(bytes / 1024 ** 2)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

export function formatSpeed(bps: number): string {
  return `${formatBytes(bps)}/s`;
}

/** 01:05 ou 1:02:05 */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

/** "há 5 min", "ontem", "12/03/2026" */
export function formatWhen(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const diffMin = Math.round((Date.now() - d.getTime()) / 60000);
  if (diffMin < 1) return "agora há pouco";
  if (diffMin < 60) return `há ${diffMin} min`;
  const diffH = Math.round(diffMin / 60);
  if (diffH < 24) return `há ${diffH} h`;
  const yesterday = new Date(Date.now() - 86400000).toDateString() === d.toDateString();
  if (yesterday) return "ontem";
  return d.toLocaleDateString("pt-BR");
}

export function formatDateTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString("pt-BR", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export const STATUS_LABEL: Record<string, string> = {
  gravando: "Gravando",
  gravacao_interrompida: "Gravação recuperada",
  pronta: "Aguardando transcrição",
  transcrevendo: "Transcrevendo",
  transcrita: "Transcrita",
  analisando: "Analisando",
  analisada: "Análise pronta",
  erro: "Precisa de atenção",
};

export const ITEM_LABEL: Record<string, string> = {
  decisao: "Decisão",
  tarefa: "Tarefa",
  pendencia: "Pendência",
  pergunta: "Pergunta",
  risco: "Risco",
  momento: "Momento importante",
};

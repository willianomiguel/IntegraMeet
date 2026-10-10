// Detecta trechos que o whisper marca como som e não como fala,
// por exemplo "[barulho]", "(música)", "*risos*", "[BLANK_AUDIO]" ou "♪".

const NOISE_WORDS = new Set([
  "barulho",
  "barulhos",
  "ruido",
  "ruidos",
  "som",
  "sons",
  "musica",
  "music",
  "aplauso",
  "aplausos",
  "applause",
  "risos",
  "risada",
  "riso",
  "laughter",
  "laughs",
  "tosse",
  "cough",
  "coughing",
  "silencio",
  "silence",
  "blank audio",
  "inaudivel",
  "inaudible",
  "noise",
  "sound",
  "sirene",
  "buzina",
  "passos",
  "batidas",
  "batida",
  "estatica",
  "chiado",
  "eco",
  "indistinto",
  "fala indistinta",
  "background noise",
  "background music",
]);

function normalize(text: string): string {
  return text
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/_/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase();
}

/** Verdadeiro quando o trecho inteiro é um marcador de som, não fala. */
export function isNoiseText(text: string): boolean {
  const t = text.trim();
  if (!t) return true;

  // Só símbolos musicais ou de som.
  if (/^[♪♫♬\s*.-]+$/.test(t)) return true;

  // Conteúdo entre [], (), ** ou ♪ que contém uma palavra de som.
  const wrapped = t.match(/^[\[(*♪♫]\s*(.+?)\s*[\])*♪♫]$/);
  if (wrapped && NOISE_WORDS.has(normalize(wrapped[1]))) return true;

  // Texto que é só uma palavra de som, mesmo sem marcador.
  if (NOISE_WORDS.has(normalize(t))) return true;

  return false;
}

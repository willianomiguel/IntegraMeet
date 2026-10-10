//! Análise da transcrição com o llama-server local (API compatível com OpenAI).
//!
//! Regras anti-invenção:
//! - Todo item precisa citar ids de segmentos reais da transcrição; sem isso é descartado.
//! - Responsável e prazo só são mantidos se aparecerem no texto dos segmentos citados.
//! - Tipos fora da lista permitida são descartados.

use std::collections::BTreeSet;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::meetings::Segment;
use crate::transcription::fmt_time;

const CHUNK_CHARS: usize = 9_000;
const TIPOS: &[&str] = &["decisao", "tarefa", "pendencia", "pergunta", "risco", "momento"];
const REQUEST_TIMEOUT: Duration = Duration::from_secs(900);

#[derive(Debug, Clone)]
pub struct ValidItem {
    pub kind: String,
    pub text: String,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub segment_ids: Vec<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct Analysis {
    pub summary: String,
    pub topics: Vec<String>,
    pub items: Vec<ValidItem>,
}

#[derive(Deserialize)]
struct RawResult {
    #[serde(default)]
    resumo: String,
    #[serde(default)]
    assuntos: Vec<String>,
    #[serde(default)]
    itens: Vec<RawItem>,
}

#[derive(Deserialize)]
struct RawItem {
    #[serde(default)]
    tipo: String,
    #[serde(default)]
    texto: String,
    #[serde(default)]
    responsavel: Option<String>,
    #[serde(default)]
    prazo: Option<String>,
    #[serde(default)]
    segmentos: Vec<i64>,
}

/// Agrupa segmentos em blocos de até `CHUNK_CHARS` caracteres, com ids visíveis ao modelo.
pub fn build_chunks(segments: &[Segment]) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for s in segments {
        let line = format!(
            "[id {}] {} ({}): {}\n",
            s.id,
            fmt_time(s.start_ms),
            s.source,
            s.text
        );
        if !current.is_empty() && current.len() + line.len() > CHUNK_CHARS {
            chunks.push(std::mem::take(&mut current));
        }
        current.push_str(&line);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

pub fn system_prompt() -> &'static str {
    "Você analisa transcrições de reuniões em português. Use somente o que está explicitamente \
     escrito na transcrição. Não invente nomes, datas, decisões ou tarefas. Se algo não estiver \
     claro, não inclua. Responda somente com um objeto JSON válido."
}

pub fn user_prompt(chunk: &str) -> String {
    format!(
        "Transcrição (cada linha começa com o id do segmento):\n\n{chunk}\n\n\
         Devolva um JSON com este formato:\n\
         {{\n\
           \"resumo\": \"resumo fiel em 3 a 6 frases\",\n\
           \"assuntos\": [\"assunto principal\"],\n\
           \"itens\": [\n\
             {{\"tipo\": \"decisao|tarefa|pendencia|pergunta|risco|momento\", \
               \"texto\": \"o que foi dito, em uma frase\", \
               \"responsavel\": \"nome exatamente como citado ou null\", \
               \"prazo\": \"prazo exatamente como citado ou null\", \
               \"segmentos\": [ids dos segmentos que sustentam o item]}}\n\
           ]\n\
         }}\n\
         Use \"momento\" para trechos importantes que não são decisão, tarefa, pendência, pergunta ou risco."
    )
}

/// Valida a resposta do modelo para o bloco. `valid_ids` são os ids que existem nesse bloco.
pub fn validate(raw: &str, segments: &[Segment]) -> Result<Analysis, String> {
    let json_text = extract_json(raw).ok_or("resposta sem JSON")?;
    let parsed: RawResult = serde_json::from_str(json_text).map_err(|e| format!("JSON inválido: {e}"))?;

    let mut items = Vec::new();
    for it in parsed.itens {
        let kind = it.tipo.trim().to_lowercase();
        if !TIPOS.contains(&kind.as_str()) {
            continue;
        }
        let text = it.texto.trim().to_string();
        if text.is_empty() {
            continue;
        }
        // Só aceita ids que existem de fato neste bloco.
        let cited: Vec<&Segment> = segments
            .iter()
            .filter(|s| it.segmentos.contains(&s.id))
            .collect();
        if cited.is_empty() {
            continue;
        }
        let evidence: String = cited.iter().map(|s| s.text.to_lowercase()).collect::<Vec<_>>().join(" ");
        let keep_if_cited = |v: Option<String>| {
            v.map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty() && x.to_lowercase() != "null")
                .filter(|x| evidence.contains(&x.to_lowercase()))
        };
        items.push(ValidItem {
            kind,
            text,
            owner: keep_if_cited(it.responsavel),
            due: keep_if_cited(it.prazo),
            segment_ids: cited.iter().map(|s| s.id).collect::<BTreeSet<_>>().into_iter().collect(),
        });
    }

    let topics = parsed
        .assuntos
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .take(8)
        .collect();

    Ok(Analysis {
        summary: parsed.resumo.trim().to_string(),
        topics,
        items,
    })
}

/// Junta resultados de vários blocos, sem repetir itens com o mesmo texto.
pub fn merge(parts: Vec<Analysis>) -> Analysis {
    let mut out = Analysis::default();
    let mut seen = BTreeSet::new();
    let mut summaries = Vec::new();
    for p in parts {
        if !p.summary.is_empty() {
            summaries.push(p.summary);
        }
        for t in p.topics {
            if !out.topics.contains(&t) && out.topics.len() < 12 {
                out.topics.push(t);
            }
        }
        for item in p.items {
            if seen.insert((item.kind.clone(), item.text.to_lowercase())) {
                out.items.push(item);
            }
        }
    }
    out.summary = summaries.join(" ");
    out
}

/// Remove cercas de código e pega o primeiro objeto JSON da resposta.
fn extract_json(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    (end > start).then(|| &raw[start..=end])
}

/// Chama o llama-server e devolve o conteúdo da primeira escolha.
pub async fn chat(http: &reqwest::Client, base_url: &str, system: &str, user: &str) -> Result<String, String> {
    let body = json!({
        "model": "local",
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0.1,
        "max_tokens": 2048,
        "response_format": {"type": "json_object"}
    });
    let resp = http
        .post(format!("{base_url}/v1/chat/completions"))
        .timeout(REQUEST_TIMEOUT)
        .json(&body)
        .send()
        .await
        .map_err(|_| "A inteligência local não respondeu. Tente novamente.".to_string())?;
    if !resp.status().is_success() {
        return Err("A inteligência local recusou a solicitação. Tente novamente.".into());
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|_| "Resposta inesperada da inteligência local.".to_string())?;
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Resposta vazia da inteligência local.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(id: i64, text: &str) -> Segment {
        Segment {
            id,
            source: "microfone".into(),
            start_ms: id * 1000,
            end_ms: id * 1000 + 900,
            text: text.into(),
            confidence: None,
        }
    }

    #[test]
    fn drops_items_without_valid_evidence() {
        let segs = vec![seg(1, "Vamos contratar a Ana até sexta."), seg(2, "Ok.")];
        let raw = r#"{"resumo":"Contratação.","assuntos":["RH"],"itens":[
            {"tipo":"tarefa","texto":"Contratar Ana","responsavel":"Ana","prazo":"sexta","segmentos":[1]},
            {"tipo":"decisao","texto":"Inventada","segmentos":[99]},
            {"tipo":"inventado","texto":"x","segmentos":[1]}
        ]}"#;
        let a = validate(raw, &segs).unwrap();
        assert_eq!(a.items.len(), 1);
        assert_eq!(a.items[0].owner.as_deref(), Some("Ana"));
        assert_eq!(a.items[0].due.as_deref(), Some("sexta"));
    }

    #[test]
    fn nulls_owner_not_present_in_transcript() {
        let segs = vec![seg(1, "Alguém precisa revisar o contrato.")];
        let raw = r#"{"resumo":"","itens":[{"tipo":"tarefa","texto":"Revisar contrato","responsavel":"Maria","prazo":null,"segmentos":[1]}]}"#;
        let a = validate(raw, &segs).unwrap();
        assert_eq!(a.items[0].owner, None);
    }

    #[test]
    fn extracts_json_from_code_fences() {
        let segs = vec![seg(1, "teste")];
        let raw = "```json\n{\"resumo\":\"ok\",\"itens\":[]}\n```";
        assert_eq!(validate(raw, &segs).unwrap().summary, "ok");
    }

    #[test]
    fn chunks_respect_size_limit() {
        let big = "palavra ".repeat(200);
        let segs: Vec<Segment> = (0..30).map(|i| seg(i, &big)).collect();
        let chunks = build_chunks(&segs);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.len() <= CHUNK_CHARS + 400));
    }
}

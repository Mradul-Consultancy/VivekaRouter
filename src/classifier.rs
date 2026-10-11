use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sha2::{Sha256, Digest};

use crate::models::{
    Classification, Complexity, TaskType,
    AnthropicRequest, AnthropicResponse, OpenAIMessage,
    HAIKU, ANTHROPIC_API,
};

const CACHE_TTL_SECS: u64 = 3600;

#[derive(Clone)]
struct CacheEntry {
    classification: Classification,
    inserted_at:    Instant,
    embedding:      Option<Vec<f32>>,
}

#[derive(Clone)]
pub struct Classifier {
    api_key: String,
    client:  reqwest::Client,
    cache:   Arc<Mutex<HashMap<String, CacheEntry>>>,
}

impl Classifier {
    pub fn new(api_key: String) -> Self {
        Self { api_key, client: reqwest::Client::new(), cache: Arc::new(Mutex::new(HashMap::new())) }
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() { return 0.0; }
        let mut dot = 0.0f32;
        let mut norm_a = 0.0f32;
        let mut norm_b = 0.0f32;
        for i in 0..a.len() {
            dot += a[i] * b[i];
            norm_a += a[i] * a[i];
            norm_b += b[i] * b[i];
        }
        if norm_a == 0.0 || norm_b == 0.0 { return 0.0; }
        dot / (norm_a.sqrt() * norm_b.sqrt())
    }

    fn quick_text_embedding(text: &str) -> Vec<f32> {
        let mut emb = vec![0.0f32; 128];
        let mut sum_sq = 0.0f32;
        for (i, byte) in text.as_bytes().iter().enumerate() {
            let idx = (i * 31 + *byte as usize) % 128;
            emb[idx] += 1.0;
        }
        for v in &emb { sum_sq += v * v; }
        if sum_sq > 0.0 {
            let norm = sum_sq.sqrt();
            for v in emb.iter_mut() { *v /= norm; }
        }
        emb
    }

    fn prompt_hash(text: &str) -> String {
        let mut hasher = Sha256::new();
        // Hash prefix + suffix + length to avoid collisions on templated
        // prompts that share a common preamble but differ in the actual task.
        let prefix = &text[..text.len().min(300)];
        let suffix_start = text.len().saturating_sub(200);
        let suffix = &text[suffix_start..];
        hasher.update(prefix.as_bytes());
        hasher.update(suffix.as_bytes());
        hasher.update(text.len().to_le_bytes());
        hex::encode(hasher.finalize())
    }

    fn get_cached(&self, hash: &str, text: &str) -> Option<Classification> {
        let mut cache = self.cache.lock().unwrap();
        // Exact SHA-256 match
        if let Some(entry) = cache.get(hash) {
            if entry.inserted_at.elapsed() < Duration::from_secs(CACHE_TTL_SECS) {
                let mut c = entry.classification.clone();
                c.cache_hit = true;
                return Some(c);
            }
            cache.remove(hash);
        }

        // Semantic Vector Similarity Match (threshold > 0.95)
        let query_emb = Self::quick_text_embedding(text);
        for entry in cache.values() {
            if entry.inserted_at.elapsed() < Duration::from_secs(CACHE_TTL_SECS) {
                if let Some(ref emb) = entry.embedding {
                    if Self::cosine_similarity(&query_emb, emb) >= 0.96 {
                        let mut c = entry.classification.clone();
                        c.cache_hit = true;
                        c.reasoning = format!("Semantic Cache Hit (~96%+ similarity): {}", c.reasoning);
                        return Some(c);
                    }
                }
            }
        }

        None
    }

    fn set_cached(&self, hash: String, text: &str, c: Classification) {
        let mut cache = self.cache.lock().unwrap();
        let emb = Some(Self::quick_text_embedding(text));
        cache.insert(hash, CacheEntry { classification: c, inserted_at: Instant::now(), embedding: emb });
        if cache.len() > 10_000 {
            cache.retain(|_, v| v.inserted_at.elapsed() < Duration::from_secs(CACHE_TTL_SECS));
        }
    }

    fn extract_prompt_text(messages: &[OpenAIMessage]) -> String {
        messages.iter().rev().find_map(|m| {
            match &m.content {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Array(arr) => arr.iter().find_map(|v| {
                    if v.get("type").and_then(|t| t.as_str()) == Some("text") {
                        v.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                    } else { None }
                }),
                _ => None,
            }
        }).unwrap_or_default()
    }

    fn fast_path_heuristic(text: &str) -> Option<Classification> {
        let trimmed = text.trim();
        let lower = trimmed.to_lowercase();
        let word_count = trimmed.split_whitespace().count();

        // 1. Trivial greetings & acknowledgements -> Simple (Haiku)
        let simple_greetings = ["hi", "hello", "hey", "sup", "good morning", "good evening", "thanks", "thank you", "ok", "okay", "bye"];
        if word_count <= 4 && simple_greetings.iter().any(|g| lower == *g || lower.starts_with(g)) {
            return Some(Classification {
                complexity: Complexity::Simple,
                task_type: TaskType::Other,
                estimated_output_tokens: 64,
                reasoning: "Fast-path: trivial greeting / conversational acknowledgement".to_string(),
                routed_to: crate::models::HAIKU.to_string(),
                cache_hit: false,
            });
        }

        // 2. Simple formatting / case transform / JSON validation tasks
        if (lower.contains("convert to json") || lower.contains("format as json") || lower.contains("to uppercase") || lower.contains("to lowercase")) && word_count < 50 {
            return Some(Classification {
                complexity: Complexity::Simple,
                task_type: TaskType::Data,
                estimated_output_tokens: 256,
                reasoning: "Fast-path: deterministic data formatting request".to_string(),
                routed_to: crate::models::HAIKU.to_string(),
                cache_hit: false,
            });
        }

        // 3. Clear indicators of complex architectural / algorithmic design -> Complex (Fable/Opus)
        let complex_triggers = [
            "distributed consensus", "raft algorithm", "byzantine fault",
            "microservices architecture design", "zero-knowledge proof", "formal verification",
            "system design document", "design a high throughput distributed"
        ];
        if complex_triggers.iter().any(|t| lower.contains(t)) || (word_count > 600 && (lower.contains("architecture") || lower.contains("benchmark"))) {
            return Some(Classification {
                complexity: Complexity::Complex,
                task_type: TaskType::Code,
                estimated_output_tokens: 2048,
                reasoning: "Fast-path: high-complexity system design or algorithmic task".to_string(),
                routed_to: crate::models::FABLE.to_string(),
                cache_hit: false,
            });
        }

        None
    }

    pub async fn classify(&self, messages: &[OpenAIMessage]) -> Classification {
        let prompt_text = Self::extract_prompt_text(messages);
        let hash = Self::prompt_hash(&prompt_text);

        if let Some(cached) = self.get_cached(&hash, &prompt_text) {
            return cached;
        }

        // Fast-path heuristic pre-routing: 0ms latency and $0 token spend
        if let Some(fast_class) = Self::fast_path_heuristic(&prompt_text) {
            self.set_cached(hash, &prompt_text, fast_class.clone());
            return fast_class;
        }

        let system = "You are a task classifier. Classify the user's request.
Return ONLY valid JSON with exactly these fields:
{
  \"complexity\": \"simple\" | \"medium\" | \"complex\",
  \"task_type\": \"code\" | \"research\" | \"creative\" | \"data\" | \"other\",
  \"estimated_output_tokens\": <integer 1-4096>,
  \"reasoning\": \"<one sentence>\"
}

Guidelines:
- simple:  greetings, single facts, trivial lookups, short questions
- medium:  standard coding tasks, summaries, reviews, analysis, writing
- complex: architecture design, novel algorithms, long-horizon reasoning, multi-step debugging
";

        let snippet = &prompt_text[..prompt_text.len().min(500)];
        let user_msg = format!("Classify this task:\n\n{snippet}");

        let req = AnthropicRequest {
            model: HAIKU.to_string(),
            messages: vec![OpenAIMessage { role: "user".to_string(), content: serde_json::Value::String(user_msg) }],
            max_tokens:  256,
            temperature: Some(0.0),
            system:      Some(system.to_string()),
        };

        match self.call_anthropic(&req).await {
            Ok(resp) => {
                let text = resp.content.iter().find(|b| b.block_type == "text")
                    .and_then(|b| b.text.as_ref()).cloned().unwrap_or_default();
                let parsed = self.parse_classification(&text);
                let routed = parsed.route_model().to_string();
                let mut result = parsed;
                result.routed_to = routed;
                result.cache_hit = false;
                self.set_cached(hash, &prompt_text, result.clone());
                result
            }
            Err(e) => {
                tracing::warn!("Classifier failed: {e}. Defaulting to medium/Sonnet.");
                Classification {
                    complexity: Complexity::Medium,
                    task_type:  TaskType::Other,
                    estimated_output_tokens: 512,
                    reasoning:  format!("Classifier error: {e}. Defaulted to medium."),
                    routed_to:  crate::models::SONNET.to_string(),
                    cache_hit:  false,
                }
            }
        }
    }

    fn parse_classification(&self, text: &str) -> Classification {
        let clean = text.replace("```json", "").replace("```", "").trim().to_string();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&clean) {
            let complexity = match v.get("complexity").and_then(|c| c.as_str()) {
                Some("simple")  => Complexity::Simple,
                Some("complex") => Complexity::Complex,
                _               => Complexity::Medium,
            };
            let task_type = match v.get("task_type").and_then(|t| t.as_str()) {
                Some("code")     => TaskType::Code,
                Some("research") => TaskType::Research,
                Some("creative") => TaskType::Creative,
                Some("data")     => TaskType::Data,
                _                => TaskType::Other,
            };
            let tokens = v.get("estimated_output_tokens").and_then(|t| t.as_i64()).unwrap_or(512);
            let reasoning = v.get("reasoning").and_then(|r| r.as_str()).unwrap_or("Classified by Haiku").to_string();
            Classification { complexity, task_type, estimated_output_tokens: tokens, reasoning, routed_to: String::new(), cache_hit: false }
        } else {
            Classification {
                complexity: Complexity::Medium, task_type: TaskType::Other,
                estimated_output_tokens: 512,
                reasoning: "JSON parse failed; defaulted to medium".to_string(),
                routed_to: String::new(), cache_hit: false,
            }
        }
    }

    pub async fn call_anthropic(&self, req: &AnthropicRequest) -> Result<AnthropicResponse, reqwest::Error> {
        self.client.post(ANTHROPIC_API)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(req).send().await?.json::<AnthropicResponse>().await
    }
}

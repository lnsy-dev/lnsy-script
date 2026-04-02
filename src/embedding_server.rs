use rquickjs::{class::Trace, Ctx, JsLifetime};
use ort::session::Session;
use ort::value::Tensor;
use std::sync::{Arc, Mutex};

// ── Embedded model bytes ──────────────────────────────────────────────────────

static MINILM_ONNX: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/minilm/model.onnx"));
static MINILM_TOKENIZER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/minilm/tokenizer.json"));
static MINILM_TOKENIZER_CONFIG: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/minilm/tokenizer_config.json"));
static MINILM_SPECIAL_TOKENS: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/minilm/special_tokens_map.json"));
static MINILM_CONFIG: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/minilm/config.json"));

static QA_ONNX: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/qa/model_int8.onnx"));
static QA_TOKENIZER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/qa/tokenizer.json"));

static SENTIMENT_ONNX: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/sentiment/model_int8.onnx"));
static SENTIMENT_TOKENIZER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/sentiment/tokenizer.json"));
static SENTIMENT_CONFIG: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/sentiment/config.json"));

static NER_ONNX: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/ner/model_quantized.onnx"));
static NER_TOKENIZER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/ner/tokenizer.json"));
static NER_CONFIG: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/models/ner/config.json"));

// ── Model holder ─────────────────────────────────────────────────────────────

struct Models {
    embedding: fastembed::TextEmbedding,
    qa: Session,
    qa_tokenizer: tokenizers::Tokenizer,
    sentiment: Session,
    sentiment_tokenizer: tokenizers::Tokenizer,
    sentiment_labels: Vec<String>,
    ner: Session,
    ner_tokenizer: tokenizers::Tokenizer,
    ner_labels: Vec<String>,
}

fn ort_err(msg: String) -> rquickjs::Error {
    rquickjs::Error::new_from_js_message("error", "ort", msg)
}

fn init_models(custom_onnx: Option<&str>) -> Result<Models, String> {
    use fastembed::{InitOptionsUserDefined, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel};

    let tokenizer_files = TokenizerFiles {
        tokenizer_file: MINILM_TOKENIZER.to_vec(),
        config_file: MINILM_CONFIG.to_vec(),
        special_tokens_map_file: MINILM_SPECIAL_TOKENS.to_vec(),
        tokenizer_config_file: MINILM_TOKENIZER_CONFIG.to_vec(),
    };

    let embedding = if let Some(path) = custom_onnx {
        let onnx_bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let model = UserDefinedEmbeddingModel::new(onnx_bytes, tokenizer_files);
        TextEmbedding::try_new_from_user_defined(model, InitOptionsUserDefined::default())
            .map_err(|e| e.to_string())?
    } else {
        let model = UserDefinedEmbeddingModel::new(MINILM_ONNX.to_vec(), tokenizer_files);
        TextEmbedding::try_new_from_user_defined(model, InitOptionsUserDefined::default())
            .map_err(|e| e.to_string())?
    };

    let qa = Session::builder()
        .map_err(|e| e.to_string())?
        .commit_from_memory(QA_ONNX)
        .map_err(|e| e.to_string())?;
    let qa_tokenizer =
        tokenizers::Tokenizer::from_bytes(QA_TOKENIZER).map_err(|e| e.to_string())?;

    let sentiment = Session::builder()
        .map_err(|e| e.to_string())?
        .commit_from_memory(SENTIMENT_ONNX)
        .map_err(|e| e.to_string())?;
    let sentiment_tokenizer =
        tokenizers::Tokenizer::from_bytes(SENTIMENT_TOKENIZER).map_err(|e| e.to_string())?;
    let sentiment_labels = extract_id2label(SENTIMENT_CONFIG)?;

    let ner = Session::builder()
        .map_err(|e| e.to_string())?
        .commit_from_memory(NER_ONNX)
        .map_err(|e| e.to_string())?;
    let ner_tokenizer =
        tokenizers::Tokenizer::from_bytes(NER_TOKENIZER).map_err(|e| e.to_string())?;
    let ner_labels = extract_id2label(NER_CONFIG)?;

    Ok(Models {
        embedding,
        qa,
        qa_tokenizer,
        sentiment,
        sentiment_tokenizer,
        sentiment_labels,
        ner,
        ner_tokenizer,
        ner_labels,
    })
}

fn extract_id2label(config_bytes: &[u8]) -> Result<Vec<String>, String> {
    let v: serde_json::Value =
        serde_json::from_slice(config_bytes).map_err(|e| e.to_string())?;
    let map = v
        .get("id2label")
        .and_then(|m| m.as_object())
        .ok_or("no id2label in config")?;
    let max_id = map
        .keys()
        .filter_map(|k| k.parse::<usize>().ok())
        .max()
        .unwrap_or(0);
    let mut labels = vec![String::new(); max_id + 1];
    for (k, v) in map {
        if let (Ok(i), Some(s)) = (k.parse::<usize>(), v.as_str()) {
            if i < labels.len() {
                labels[i] = s.to_string();
            }
        }
    }
    Ok(labels)
}

fn make_i64_tensor(data: Vec<i64>, seq_len: usize) -> rquickjs::Result<Tensor<i64>> {
    Tensor::<i64>::from_array(([1usize, seq_len], data)).map_err(|e| ort_err(e.to_string()))
}

// ── EmbeddingServer class ─────────────────────────────────────────────────────

#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct EmbeddingServer {
    #[qjs(skip_trace)]
    inner: Arc<Mutex<Models>>,
}

#[rquickjs::methods]
impl EmbeddingServer {
    #[qjs(constructor)]
    pub fn new(model_path: rquickjs::prelude::Opt<String>) -> rquickjs::Result<Self> {
        let custom = model_path.0.as_deref().map(str::to_string);
        let models = init_models(custom.as_deref())
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "EmbeddingServer init", e))?;
        Ok(EmbeddingServer {
            inner: Arc::new(Mutex::new(models)),
        })
    }

    #[qjs(rename = "__generateEmbeddingSync")]
    pub fn generate_embedding_sync(&self, text: String) -> rquickjs::Result<String> {
        let mut m = self.inner.lock().unwrap();
        let vecs = m
            .embedding
            .embed(vec![text], None)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "embed", e.to_string()))?;
        let vec = vecs.into_iter().next().unwrap_or_default();
        serde_json::to_string(&vec)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "json", e.to_string()))
    }

    #[qjs(rename = "__askQuestionSync")]
    pub fn ask_question_sync(&self, context_text: String, question: String) -> rquickjs::Result<String> {
        let mut m = self.inner.lock().unwrap();

        let encoding = m
            .qa_tokenizer
            .encode(
                tokenizers::EncodeInput::Dual(
                    question.as_str().into(),
                    context_text.as_str().into(),
                ),
                true,
            )
            .map_err(|e: tokenizers::Error| ort_err(e.to_string()))?;

        let seq_len = encoding.get_ids().len();
        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let attention_mask: Vec<i64> =
            encoding.get_attention_mask().iter().map(|&x| x as i64).collect();
        let token_type_ids: Vec<i64> =
            encoding.get_type_ids().iter().map(|&x| x as i64).collect();

        let ids_t = make_i64_tensor(input_ids, seq_len)?;
        let mask_t = make_i64_tensor(attention_mask, seq_len)?;

        // DistilBERT does not use token_type_ids
        let _ = token_type_ids;

        let inputs = ort::inputs![
            "input_ids" => ids_t,
            "attention_mask" => mask_t
        ];
        let outputs = m.qa.run(inputs).map_err(|e| ort_err(e.to_string()))?;

        let (_, start_data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e: ort::Error| ort_err(e.to_string()))?;
        let (_, end_data) = outputs[1]
            .try_extract_tensor::<f32>()
            .map_err(|e: ort::Error| ort_err(e.to_string()))?;

        let start = start_data
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);
        let end = end_data
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);

        let tokens = encoding.get_tokens();
        let answer = if start <= end && end < tokens.len() {
            tokens[start..=end]
                .iter()
                .map(|t: &String| t.trim_start_matches("##"))
                .collect::<Vec<_>>()
                .join("")
        } else {
            String::new()
        };

        Ok(answer)
    }

    #[qjs(rename = "__getSentimentSync")]
    pub fn get_sentiment_sync(&self, text: String) -> rquickjs::Result<String> {
        let mut m = self.inner.lock().unwrap();

        let encoding = m
            .sentiment_tokenizer
            .encode(text.as_str(), true)
            .map_err(|e| ort_err(e.to_string()))?;

        let seq_len = encoding.get_ids().len();
        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let attention_mask: Vec<i64> =
            encoding.get_attention_mask().iter().map(|&x| x as i64).collect();

        let ids_t = make_i64_tensor(input_ids, seq_len)?;
        let mask_t = make_i64_tensor(attention_mask, seq_len)?;

        let inputs = ort::inputs![
            "input_ids" => ids_t,
            "attention_mask" => mask_t
        ];
        let outputs = m.sentiment.run(inputs).map_err(|e| ort_err(e.to_string()))?;

        // Extract data into owned Vec so we can drop outputs before accessing m.sentiment_labels
        let row: Vec<f32> = {
            let (_, logits_data) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e: ort::Error| ort_err(e.to_string()))?;
            logits_data.iter().cloned().collect()
        };
        drop(outputs);

        let max = row.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exps: Vec<f32> = row.iter().map(|x| (x - max).exp()).collect();
        let sum: f32 = exps.iter().sum();
        let probs: Vec<f32> = exps.iter().map(|x| x / sum).collect();

        let best = probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);

        let label = m.sentiment_labels.get(best).cloned().unwrap_or_default();
        let score = probs.get(best).cloned().unwrap_or(0.0);

        Ok(format!(r#"{{"label":"{label}","score":{score:.6}}}"#))
    }

    /// One-shot classification via cosine similarity of BERT embeddings.
    ///
    /// `labels_json`   – JSON array of label strings, e.g. `["sports","politics","tech"]`
    /// `examples_json` – Optional JSON array of one representative text per label (same
    ///                   order / length as labels).  When omitted the label names themselves
    ///                   are used as the reference texts (zero-shot style).
    ///
    /// Returns JSON: `{"label":"sports","score":0.923456}`
    #[qjs(rename = "__classifyOneShotSync")]
    pub fn classify_one_shot_sync(
        &self,
        text: String,
        labels_json: String,
        examples_json: rquickjs::prelude::Opt<String>,
    ) -> rquickjs::Result<String> {
        let labels: Vec<String> = serde_json::from_str(&labels_json)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "classifyOneShot parse labels", e.to_string()))?;
        if labels.is_empty() {
            return Err(rquickjs::Error::new_from_js_message("error", "classifyOneShot", String::from("labels array must not be empty")));
        }

        let reference_texts: Vec<String> = if let Some(ex_json) = examples_json.0 {
            let ex: Vec<String> = serde_json::from_str(&ex_json)
                .map_err(|e| rquickjs::Error::new_from_js_message("error", "classifyOneShot parse examples", e.to_string()))?;
            if ex.len() != labels.len() {
                return Err(rquickjs::Error::new_from_js_message(
                    "error", "classifyOneShot",
                    format!("examples length ({}) must match labels length ({})", ex.len(), labels.len()),
                ));
            }
            ex
        } else {
            labels.clone()
        };

        let mut m = self.inner.lock().unwrap();

        // Embed query + all reference texts in a single batch
        let mut batch: Vec<String> = Vec::with_capacity(1 + reference_texts.len());
        batch.push(text);
        batch.extend(reference_texts.iter().cloned());

        let mut vecs = m
            .embedding
            .embed(batch, None)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "embed", e.to_string()))?;

        let query_vec = vecs.remove(0);
        let ref_vecs = vecs; // one per label

        let best = ref_vecs
            .iter()
            .enumerate()
            .map(|(i, rv)| (i, cosine_similarity(&query_vec, rv)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let (best_idx, best_score) = best.unwrap_or((0, 0.0));
        let best_label = &labels[best_idx];

        Ok(format!(r#"{{"label":{label_json},"score":{score:.6}}}"#,
            label_json = serde_json::to_string(best_label).unwrap(),
            score = best_score,
        ))
    }

    #[qjs(rename = "__getNamedEntitiesSync")]
    pub fn get_named_entities_sync(&self, text: String) -> rquickjs::Result<String> {
        let mut m = self.inner.lock().unwrap();

        let encoding = m
            .ner_tokenizer
            .encode(text.as_str(), true)
            .map_err(|e| ort_err(e.to_string()))?;

        let seq_len = encoding.get_ids().len();
        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let attention_mask: Vec<i64> =
            encoding.get_attention_mask().iter().map(|&x| x as i64).collect();
        let token_type_ids: Vec<i64> =
            encoding.get_type_ids().iter().map(|&x| x as i64).collect();

        let ids_t = make_i64_tensor(input_ids, seq_len)?;
        let mask_t = make_i64_tensor(attention_mask, seq_len)?;
        let types_t = make_i64_tensor(token_type_ids, seq_len)?;

        let inputs = ort::inputs![
            "input_ids" => ids_t,
            "attention_mask" => mask_t,
            "token_type_ids" => types_t
        ];
        let outputs = m.ner.run(inputs).map_err(|e| ort_err(e.to_string()))?;

        // Extract flat data and shape into owned Vecs before dropping outputs
        let (logits_flat, num_labels) = {
            let (shape, logits_data) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e: ort::Error| ort_err(e.to_string()))?;
            let nl = shape[2] as usize;
            (logits_data.iter().cloned().collect::<Vec<f32>>(), nl)
        };
        drop(outputs);
        let ner_labels = m.ner_labels.clone();

        let tokens = encoding.get_tokens();
        let mut entities: Vec<serde_json::Value> = Vec::new();
        let mut current: Option<(String, Vec<String>)> = None;

        for i in 0..seq_len {
            let token = tokens.get(i).cloned().unwrap_or_default();
            if token == "[CLS]" || token == "[SEP]" || token == "[PAD]" {
                if let Some((etype, words)) = current.take() {
                    entities.push(serde_json::json!({
                        "entity": etype,
                        "word": reconstruct(&words)
                    }));
                }
                continue;
            }

            // argmax over labels for this token (flat index: i * num_labels + j)
            let best = (0..num_labels)
                .map(|j| (j, logits_flat[i * num_labels + j]))
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|(j, _)| j)
                .unwrap_or(0);

            let label = ner_labels.get(best).cloned().unwrap_or_default();

            if label == "O" {
                if let Some((etype, words)) = current.take() {
                    entities.push(serde_json::json!({
                        "entity": etype,
                        "word": reconstruct(&words)
                    }));
                }
            } else if label.starts_with("B-") {
                if let Some((etype, words)) = current.take() {
                    entities.push(serde_json::json!({
                        "entity": etype,
                        "word": reconstruct(&words)
                    }));
                }
                current = Some((label[2..].to_string(), vec![token]));
            } else if label.starts_with("I-") {
                if let Some((_, ref mut words)) = current {
                    words.push(token);
                }
            }
        }
        if let Some((etype, words)) = current.take() {
            entities.push(serde_json::json!({"entity": etype, "word": reconstruct(&words)}));
        }

        serde_json::to_string(&entities)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "json", e.to_string()))
    }
}

fn reconstruct(tokens: &[String]) -> String {
    let mut result = String::new();
    for token in tokens {
        if token.starts_with("##") {
            result.push_str(&token[2..]);
        } else {
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(token);
        }
    }
    result
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
}

pub fn create_embedding_model() -> Result<fastembed::TextEmbedding, String> {
    use fastembed::{InitOptionsUserDefined, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel};
    let tokenizer_files = TokenizerFiles {
        tokenizer_file: MINILM_TOKENIZER.to_vec(),
        config_file: MINILM_CONFIG.to_vec(),
        special_tokens_map_file: MINILM_SPECIAL_TOKENS.to_vec(),
        tokenizer_config_file: MINILM_TOKENIZER_CONFIG.to_vec(),
    };
    let model = UserDefinedEmbeddingModel::new(MINILM_ONNX.to_vec(), tokenizer_files);
    TextEmbedding::try_new_from_user_defined(model, InitOptionsUserDefined::default())
        .map_err(|e| e.to_string())
}

// ── Registration ──────────────────────────────────────────────────────────────

pub fn setup_embedding_server(ctx: Ctx<'_>) -> rquickjs::Result<()> {
    rquickjs::Class::<EmbeddingServer>::define(&ctx.globals())?;
    ctx.eval::<(), _>(
        r#"
EmbeddingServer.prototype.generateEmbedding = function(text) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { resolve(JSON.parse(self.__generateEmbeddingSync(text))); }
        catch(e) { reject(e); }
    });
};
EmbeddingServer.prototype.askQuestion = function(context, question) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { resolve(self.__askQuestionSync(context, question)); }
        catch(e) { reject(e); }
    });
};
EmbeddingServer.prototype.getSentiment = function(text) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { resolve(JSON.parse(self.__getSentimentSync(text))); }
        catch(e) { reject(e); }
    });
};
EmbeddingServer.prototype.getNamedEntities = function(text) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { resolve(JSON.parse(self.__getNamedEntitiesSync(text))); }
        catch(e) { reject(e); }
    });
};
EmbeddingServer.prototype.classifyOneShot = function(text, labels, examples) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try {
            var labelsJson = JSON.stringify(labels);
            var examplesJson = examples ? JSON.stringify(examples) : undefined;
            resolve(JSON.parse(self.__classifyOneShotSync(text, labelsJson, examplesJson)));
        } catch(e) { reject(e); }
    });
};
"#,
    )?;
    Ok(())
}

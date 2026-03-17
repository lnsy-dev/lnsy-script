use rquickjs::{class::Trace, Ctx, JsLifetime};
use std::cell::RefCell;
use serde::{Deserialize, Serialize};

use intellichip_rs::TRMConfig;
use intellichip_rs::models::loader::load_model;
use intellichip_rs::data::{OpcodeDataLoader, OpcodeDataset, OpcodeExample, OpcodeVocab};
use intellichip_rs::training::{Trainer, TrainingConfig};
use candle_core::Device;

fn trm_err(msg: impl Into<String>) -> rquickjs::Error {
    rquickjs::Error::new_from_js_message("error", "TRM", msg.into())
}

struct TrainingItem {
    text: String,
    label: String,
}

struct TRMInference {
    model: intellichip_rs::TinyRecursiveModel,
    vocab: OpcodeVocab,
    device: Device,
    max_seq_len: usize,
}

#[derive(Serialize, Deserialize)]
struct SavedConfig {
    config: TRMConfig,
    max_seq_len: usize,
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct TRM {
    #[qjs(skip_trace)]
    data: RefCell<Vec<TrainingItem>>,
    #[qjs(skip_trace)]
    dir_path: Option<String>,
    #[qjs(skip_trace)]
    inference: RefCell<Option<TRMInference>>,
}

fn build_examples(data: &[TrainingItem]) -> Vec<OpcodeExample> {
    data.iter()
        .map(|item| OpcodeExample {
            input: item.text.clone(),
            candidates: vec![],
            target_opcode: item.label.clone(),
            confidence_target: 1.0,
            domain: String::new(),
            source: String::new(),
            version: String::new(),
        })
        .collect()
}

fn train_model(data: &[TrainingItem], dir: &str) -> rquickjs::Result<TRMInference> {
    if data.is_empty() {
        return Err(trm_err("no training data"));
    }

    let unique_labels: std::collections::HashSet<&str> =
        data.iter().map(|d| d.label.as_str()).collect();
    if unique_labels.len() < 2 {
        return Err(trm_err("need at least 2 distinct labels for training"));
    }

    let examples = build_examples(data);
    let vocab = OpcodeVocab::from_examples(&examples);
    let vocab_size = vocab.vocab_size();
    let num_opcodes = vocab.num_opcodes();

    let max_seq_len = data
        .iter()
        .map(|d| d.text.len())
        .max()
        .unwrap_or(32)
        .clamp(32, 256);

    std::fs::create_dir_all(dir).map_err(|e| trm_err(e.to_string()))?;

    // Write temp JSONL for OpcodeDataset::from_jsonl
    let jsonl_path = format!("{}/trm_training_temp.jsonl", dir);
    let mut jsonl_content = String::new();
    for ex in &examples {
        let line = serde_json::to_string(ex).map_err(|e| trm_err(e.to_string()))?;
        jsonl_content.push_str(&line);
        jsonl_content.push('\n');
    }
    std::fs::write(&jsonl_path, &jsonl_content).map_err(|e| trm_err(e.to_string()))?;

    let dataset = OpcodeDataset::from_jsonl(&jsonl_path, max_seq_len)
        .map_err(|e| trm_err(e.to_string()))?;
    std::fs::remove_file(&jsonl_path).ok();

    let mut dataloader = OpcodeDataLoader::new(dataset, 8, true);

    let model_config = TRMConfig {
        vocab_size,
        num_outputs: num_opcodes,
        hidden_size: 128,
        h_cycles: 2,
        l_cycles: 2,
        l_layers: 2,
        num_heads: 4,
        expansion: 4.0,
        pos_encodings: "rope".to_string(),
        mlp_t: false,
        halt_max_steps: 8,
        dropout: 0.1,
    };

    let training_config = TrainingConfig {
        num_epochs: 20,
        batch_size: 8,
        learning_rate: 3e-4,
        lr_min: 3e-5,
        warmup_steps: 50,
        total_steps: 5000,
        weight_decay: 0.01,
        grad_clip: Some(1.0),
        ema_decay: 0.999,
        save_every: usize::MAX,
        eval_every: usize::MAX,
        checkpoint_dir: dir.to_string(),
    };

    let device = Device::Cpu;
    let mut trainer = Trainer::new(model_config.clone(), training_config, device)
        .map_err(|e| trm_err(e.to_string()))?;

    trainer
        .train(&mut dataloader)
        .map_err(|e| trm_err(e.to_string()))?;

    let model_path = format!("{}/model.safetensors", dir);
    trainer
        .save_checkpoint(&model_path, None)
        .map_err(|e| trm_err(e.to_string()))?;

    let saved_config = SavedConfig {
        config: model_config.clone(),
        max_seq_len,
    };
    let config_json =
        serde_json::to_string(&saved_config).map_err(|e| trm_err(e.to_string()))?;
    std::fs::write(format!("{}/trm_config.json", dir), config_json)
        .map_err(|e| trm_err(e.to_string()))?;

    let model = load_model(model_config, &model_path, &Device::Cpu)
        .map_err(|e| trm_err(e.to_string()))?;

    Ok(TRMInference {
        model,
        vocab,
        device: Device::Cpu,
        max_seq_len,
    })
}

fn load_inference(dir: &str) -> rquickjs::Result<Option<(TRMInference, Vec<TrainingItem>)>> {
    let model_path = format!("{}/model.safetensors", dir);
    let config_path = format!("{}/trm_config.json", dir);
    let data_path = format!("{}/training_data.json", dir);

    if !std::path::Path::new(&model_path).exists()
        || !std::path::Path::new(&config_path).exists()
    {
        return Ok(None);
    }

    let config_str =
        std::fs::read_to_string(&config_path).map_err(|e| trm_err(e.to_string()))?;
    let saved_config: SavedConfig =
        serde_json::from_str(&config_str).map_err(|e| trm_err(e.to_string()))?;

    let training_items: Vec<TrainingItem> = if std::path::Path::new(&data_path).exists() {
        let data_str =
            std::fs::read_to_string(&data_path).map_err(|e| trm_err(e.to_string()))?;
        let items: Vec<serde_json::Value> =
            serde_json::from_str(&data_str).map_err(|e| trm_err(e.to_string()))?;
        items
            .iter()
            .filter_map(|v| {
                let text = v.get("text")?.as_str()?.to_string();
                let label = v.get("label")?.as_str()?.to_string();
                Some(TrainingItem { text, label })
            })
            .collect()
    } else {
        Vec::new()
    };

    // Reconstruct vocab from training data (deterministic — same examples → same vocab)
    let examples = build_examples(&training_items);
    let vocab = OpcodeVocab::from_examples(&examples);

    let model = load_model(saved_config.config, &model_path, &Device::Cpu)
        .map_err(|e| trm_err(e.to_string()))?;

    Ok(Some((
        TRMInference {
            model,
            vocab,
            device: Device::Cpu,
            max_seq_len: saved_config.max_seq_len,
        },
        training_items,
    )))
}

fn infer(inf: &TRMInference, text: &str) -> rquickjs::Result<Vec<(String, f32)>> {
    let token_ids = inf.vocab.tokenize(text);

    let mut ids: Vec<u32> = token_ids.into_iter().take(inf.max_seq_len).collect();
    while ids.len() < inf.max_seq_len {
        ids.push(0); // PAD
    }

    let input =
        candle_core::Tensor::from_slice(ids.as_slice(), (1usize, inf.max_seq_len), &inf.device)
            .map_err(|e| trm_err(e.to_string()))?;

    let carry = inf
        .model
        .empty_carry(1)
        .map_err(|e| trm_err(e.to_string()))?;
    let (_, logits) = inf
        .model
        .forward(&carry, &input)
        .map_err(|e| trm_err(e.to_string()))?;
    // logits: [1, max_seq_len, num_outputs]

    // Mean-pool over seq dimension → [1, num_outputs]
    let pooled = logits.mean(1).map_err(|e| trm_err(e.to_string()))?;

    // Softmax over class dimension
    let probs = candle_nn::ops::softmax(&pooled, 1).map_err(|e| trm_err(e.to_string()))?;

    let probs_vec: Vec<f32> = probs
        .squeeze(0)
        .map_err(|e| trm_err(e.to_string()))?
        .to_vec1()
        .map_err(|e| trm_err(e.to_string()))?;

    let mut results: Vec<(String, f32)> = probs_vec
        .iter()
        .enumerate()
        .filter_map(|(i, &score)| {
            inf.vocab.opcode_str(i).map(|label| (label.to_string(), score))
        })
        .collect();

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    Ok(results)
}

#[rquickjs::methods]
impl TRM {
    #[qjs(constructor)]
    pub fn new(path: rquickjs::prelude::Opt<String>) -> rquickjs::Result<Self> {
        let dir_path = path.0;
        let (inference, data) = match &dir_path {
            Some(dir) => match load_inference(dir)? {
                Some((inf, items)) => (Some(inf), items),
                None => (None, Vec::new()),
            },
            None => (None, Vec::new()),
        };
        Ok(TRM {
            data: RefCell::new(data),
            dir_path,
            inference: RefCell::new(inference),
        })
    }

    #[qjs(rename = "__trainSync")]
    pub fn train_sync(&self, data_json: String) -> rquickjs::Result<()> {
        let items: Vec<serde_json::Value> =
            serde_json::from_str(&data_json).map_err(|e| trm_err(e.to_string()))?;
        {
            let mut pts = self.data.borrow_mut();
            for v in &items {
                let text = v
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let label = v
                    .get("label")
                    .and_then(|l| l.as_str())
                    .unwrap_or("")
                    .to_string();
                if !text.is_empty() && !label.is_empty() {
                    pts.push(TrainingItem { text, label });
                }
            }
        }
        self.retrain()
    }

    #[qjs(rename = "__trainTextSync")]
    pub fn train_text_sync(
        &self,
        texts_json: String,
        labels_json: String,
    ) -> rquickjs::Result<()> {
        let texts: Vec<String> =
            serde_json::from_str(&texts_json).map_err(|e| trm_err(e.to_string()))?;
        let labels: Vec<String> =
            serde_json::from_str(&labels_json).map_err(|e| trm_err(e.to_string()))?;
        {
            let mut pts = self.data.borrow_mut();
            for (text, label) in texts.into_iter().zip(labels) {
                if !text.is_empty() && !label.is_empty() {
                    pts.push(TrainingItem { text, label });
                }
            }
        }
        self.retrain()
    }

    #[qjs(rename = "__querySync")]
    pub fn query_sync(&self, text: String, k: usize) -> rquickjs::Result<String> {
        let inf_ref = self.inference.borrow();
        let inf = inf_ref
            .as_ref()
            .ok_or_else(|| trm_err("model not trained yet — call train() first"))?;
        let results = infer(inf, &text)?;
        let top_k: Vec<serde_json::Value> = results
            .into_iter()
            .take(k)
            .map(|(label, score)| serde_json::json!({ "label": label, "score": score }))
            .collect();
        serde_json::to_string(&top_k).map_err(|e| trm_err(e.to_string()))
    }

    #[qjs(rename = "__classifySync")]
    pub fn classify_sync(&self, text: String) -> rquickjs::Result<String> {
        let inf_ref = self.inference.borrow();
        let inf = inf_ref
            .as_ref()
            .ok_or_else(|| trm_err("model not trained yet — call train() first"))?;
        let results = infer(inf, &text)?;
        Ok(results
            .into_iter()
            .next()
            .map(|(label, _)| label)
            .unwrap_or_default())
    }
}

impl TRM {
    fn retrain(&self) -> rquickjs::Result<()> {
        let dir = self
            .dir_path
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("trm_model").to_string_lossy().to_string());

        let data = self.data.borrow();
        let new_inference = train_model(&data, &dir)?;
        drop(data);

        // Save training data for future reloads
        let data = self.data.borrow();
        let training_json: Vec<serde_json::Value> = data
            .iter()
            .map(|d| serde_json::json!({"text": d.text, "label": d.label}))
            .collect();
        drop(data);

        let training_str =
            serde_json::to_string(&training_json).map_err(|e| trm_err(e.to_string()))?;
        std::fs::write(format!("{}/training_data.json", dir), training_str)
            .map_err(|e| trm_err(e.to_string()))?;

        *self.inference.borrow_mut() = Some(new_inference);
        Ok(())
    }
}

pub fn setup_trm(ctx: Ctx<'_>) -> rquickjs::Result<()> {
    rquickjs::Class::<TRM>::define(&ctx.globals())?;
    ctx.eval::<(), _>(
        r#"
TRM.prototype.train = function(data) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { self.__trainSync(JSON.stringify(data)); resolve(); }
        catch(e) { reject(e); }
    });
};
TRM.prototype.trainText = function(texts, labels) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { self.__trainTextSync(JSON.stringify(texts), JSON.stringify(labels)); resolve(); }
        catch(e) { reject(e); }
    });
};
TRM.prototype.query = function(text, k) {
    var self = this;
    if (k === undefined) k = 5;
    return new Promise(function(resolve, reject) {
        try { resolve(JSON.parse(self.__querySync(text, k))); }
        catch(e) { reject(e); }
    });
};
TRM.prototype.classify = function(text) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { resolve(self.__classifySync(text)); }
        catch(e) { reject(e); }
    });
};
"#,
    )?;
    Ok(())
}

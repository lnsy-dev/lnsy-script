use rquickjs::{class::Trace, Ctx, JsLifetime};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

struct KNNPoint {
    embedding: Vec<f32>,
    label: String,
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct KNN {
    #[qjs(skip_trace)]
    model: Arc<Mutex<fastembed::TextEmbedding>>,
    #[qjs(skip_trace)]
    points: RefCell<Vec<KNNPoint>>,
    #[qjs(skip_trace)]
    file_path: Option<String>,
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    dot / (mag_a * mag_b)
}

fn knn_err(msg: impl Into<String>) -> rquickjs::Error {
    rquickjs::Error::new_from_js_message("error", "KNN", msg.into())
}

fn load_points(path: &str) -> rquickjs::Result<Vec<KNNPoint>> {
    let data = std::fs::read_to_string(path).map_err(|e| knn_err(e.to_string()))?;
    let json: serde_json::Value = serde_json::from_str(&data).map_err(|e| knn_err(e.to_string()))?;
    let points = json
        .get("points")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let embedding = item
                        .get("embedding")?
                        .as_array()?
                        .iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect();
                    let label = item.get("label")?.as_str()?.to_string();
                    Some(KNNPoint { embedding, label })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(points)
}

fn save_points(points: &[KNNPoint], path: &str) -> rquickjs::Result<()> {
    let json_points: Vec<serde_json::Value> = points
        .iter()
        .map(|p| serde_json::json!({ "embedding": p.embedding, "label": p.label }))
        .collect();
    let json_str = serde_json::to_string(&serde_json::json!({ "points": json_points }))
        .map_err(|e| knn_err(e.to_string()))?;
    std::fs::write(path, json_str).map_err(|e| knn_err(e.to_string()))
}

fn embed_texts(model: &Arc<Mutex<fastembed::TextEmbedding>>, texts: Vec<String>) -> rquickjs::Result<Vec<Vec<f32>>> {
    let mut m = model.lock().unwrap();
    m.embed(texts, None).map_err(|e| knn_err(e.to_string()))
}

fn query_inner<'a>(points: &'a [KNNPoint], query_emb: &[f32], k: usize) -> Vec<(&'a str, f32)> {
    let mut scored: Vec<(f32, &str)> = points
        .iter()
        .map(|p| (cosine_similarity(query_emb, &p.embedding), p.label.as_str()))
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(k).map(|(score, label)| (label, score)).collect()
}

#[rquickjs::methods]
impl KNN {
    #[qjs(constructor)]
    pub fn new(path: rquickjs::prelude::Opt<String>) -> rquickjs::Result<Self> {
        let file_path = path.0;
        let points = match &file_path {
            Some(p) if std::path::Path::new(p).exists() => load_points(p)?,
            _ => Vec::new(),
        };
        let model = crate::embedding_server::create_embedding_model()
            .map_err(|e| knn_err(e))?;
        Ok(KNN {
            model: Arc::new(Mutex::new(model)),
            points: RefCell::new(points),
            file_path,
        })
    }

    #[qjs(rename = "__trainSync")]
    pub fn train_sync(&self, data_json: String) -> rquickjs::Result<()> {
        let items: Vec<serde_json::Value> =
            serde_json::from_str(&data_json).map_err(|e| knn_err(e.to_string()))?;
        let texts: Vec<String> = items
            .iter()
            .filter_map(|v| v.get("text")?.as_str().map(str::to_string))
            .collect();
        let labels: Vec<String> = items
            .iter()
            .filter_map(|v| v.get("label")?.as_str().map(str::to_string))
            .collect();
        let embeddings = embed_texts(&self.model, texts)?;
        let mut pts = self.points.borrow_mut();
        for (embedding, label) in embeddings.into_iter().zip(labels) {
            pts.push(KNNPoint { embedding, label });
        }
        if let Some(ref p) = self.file_path {
            save_points(&pts, p)?;
        }
        Ok(())
    }

    #[qjs(rename = "__trainTextSync")]
    pub fn train_text_sync(&self, texts_json: String, labels_json: String) -> rquickjs::Result<()> {
        let texts: Vec<String> =
            serde_json::from_str(&texts_json).map_err(|e| knn_err(e.to_string()))?;
        let labels: Vec<String> =
            serde_json::from_str(&labels_json).map_err(|e| knn_err(e.to_string()))?;
        let embeddings = embed_texts(&self.model, texts)?;
        let mut pts = self.points.borrow_mut();
        for (embedding, label) in embeddings.into_iter().zip(labels) {
            pts.push(KNNPoint { embedding, label });
        }
        if let Some(ref p) = self.file_path {
            save_points(&pts, p)?;
        }
        Ok(())
    }

    #[qjs(rename = "__querySync")]
    pub fn query_sync(&self, text: String, k: usize) -> rquickjs::Result<String> {
        let embeddings = embed_texts(&self.model, vec![text])?;
        let query_emb = embeddings.into_iter().next().unwrap_or_default();
        let pts = self.points.borrow();
        let results = query_inner(&pts, &query_emb, k);
        let json: Vec<serde_json::Value> = results
            .iter()
            .map(|(label, score)| serde_json::json!({ "label": label, "score": score }))
            .collect();
        serde_json::to_string(&json).map_err(|e| knn_err(e.to_string()))
    }

    #[qjs(rename = "__classifySync")]
    pub fn classify_sync(&self, text: String, k: usize) -> rquickjs::Result<String> {
        let embeddings = embed_texts(&self.model, vec![text])?;
        let query_emb = embeddings.into_iter().next().unwrap_or_default();
        let pts = self.points.borrow();
        let neighbors = query_inner(&pts, &query_emb, k);
        let mut votes: HashMap<&str, usize> = HashMap::new();
        for (label, _) in &neighbors {
            *votes.entry(label).or_insert(0) += 1;
        }
        let winner = votes
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(label, _)| label.to_string())
            .unwrap_or_default();
        Ok(winner)
    }
}

pub fn setup_knn(ctx: Ctx<'_>) -> rquickjs::Result<()> {
    rquickjs::Class::<KNN>::define(&ctx.globals())?;
    ctx.eval::<(), _>(
        r#"
KNN.prototype.train = function(data) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { self.__trainSync(JSON.stringify(data)); resolve(); }
        catch(e) { reject(e); }
    });
};
KNN.prototype.trainText = function(texts, labels) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { self.__trainTextSync(JSON.stringify(texts), JSON.stringify(labels)); resolve(); }
        catch(e) { reject(e); }
    });
};
KNN.prototype.query = function(text, k) {
    var self = this;
    if (k === undefined) k = 5;
    return new Promise(function(resolve, reject) {
        try { resolve(JSON.parse(self.__querySync(text, k))); }
        catch(e) { reject(e); }
    });
};
KNN.prototype.classify = function(text, k) {
    var self = this;
    if (k === undefined) k = 5;
    return new Promise(function(resolve, reject) {
        try { resolve(self.__classifySync(text, k)); }
        catch(e) { reject(e); }
    });
};
"#,
    )?;
    Ok(())
}

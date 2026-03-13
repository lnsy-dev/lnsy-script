use rquickjs::{class::Trace, Ctx, JsLifetime};
use std::cell::RefCell;

struct VectorItem {
    embedding: Vec<f32>,
    metadata: serde_json::Value,
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct VectorDatabase {
    #[qjs(skip_trace)]
    items: RefCell<Vec<VectorItem>>,
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

fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f32>().sqrt()
}

fn manhattan_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum()
}

fn parse_embedding(json: &str) -> rquickjs::Result<Vec<f32>> {
    let val: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| rquickjs::Error::new_from_js_message("error", "VectorDatabase", e.to_string()))?;
    val.as_array()
        .ok_or_else(|| rquickjs::Error::new_from_js_message("error", "VectorDatabase", "embedding must be an array".to_string()))
        .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
}

fn load_from_file(path: &str) -> rquickjs::Result<Vec<VectorItem>> {
    let data = std::fs::read_to_string(path)
        .map_err(|e| rquickjs::Error::new_from_js_message("error", "VectorDatabase", e.to_string()))?;
    let json: serde_json::Value = serde_json::from_str(&data)
        .map_err(|e| rquickjs::Error::new_from_js_message("error", "VectorDatabase", e.to_string()))?;
    let items = json.get("items")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter().filter_map(|item| {
                let embedding = item.get("embedding")?
                    .as_array()?
                    .iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect();
                let metadata = item.get("metadata")?.clone();
                Some(VectorItem { embedding, metadata })
            }).collect()
        })
        .unwrap_or_default();
    Ok(items)
}

fn save_to_file(items: &[VectorItem], path: &str) -> rquickjs::Result<()> {
    let items_json: Vec<serde_json::Value> = items.iter().map(|item| {
        serde_json::json!({ "embedding": item.embedding, "metadata": item.metadata })
    }).collect();
    let json_str = serde_json::to_string(&serde_json::json!({ "items": items_json }))
        .map_err(|e| rquickjs::Error::new_from_js_message("error", "VectorDatabase", e.to_string()))?;
    std::fs::write(path, json_str)
        .map_err(|e| rquickjs::Error::new_from_js_message("error", "VectorDatabase", e.to_string()))
}

#[rquickjs::methods]
impl VectorDatabase {
    #[qjs(constructor)]
    pub fn new(path: rquickjs::prelude::Opt<String>) -> rquickjs::Result<Self> {
        let file_path = path.0;
        let items = match &file_path {
            Some(p) if std::path::Path::new(p).exists() => load_from_file(p)?,
            _ => Vec::new(),
        };
        Ok(VectorDatabase { items: RefCell::new(items), file_path })
    }

    #[qjs(rename = "__addItemSync")]
    pub fn add_item_sync(&self, embedding_json: String, metadata_json: String) -> rquickjs::Result<()> {
        let embedding = parse_embedding(&embedding_json)?;
        let metadata: serde_json::Value = serde_json::from_str(&metadata_json)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "addItem", e.to_string()))?;
        self.items.borrow_mut().push(VectorItem { embedding, metadata });
        if let Some(ref path) = self.file_path {
            save_to_file(&self.items.borrow(), path)?;
        }
        Ok(())
    }

    #[qjs(rename = "__querySync")]
    pub fn query_sync(&self, embedding_json: String, count: usize, metric: String) -> rquickjs::Result<String> {
        let query = parse_embedding(&embedding_json)?;
        let items = self.items.borrow();

        let use_cosine = matches!(metric.as_str(), "cosine" | "");
        let use_euclidean = matches!(metric.as_str(), "euclidean" | "euclidian");

        let mut scored: Vec<(f32, &serde_json::Value)> = items.iter().map(|item| {
            let score = if use_cosine {
                cosine_similarity(&query, &item.embedding)
            } else if use_euclidean {
                euclidean_distance(&query, &item.embedding)
            } else {
                manhattan_distance(&query, &item.embedding)
            };
            (score, &item.metadata)
        }).collect();

        // cosine: higher = better; euclidean/manhattan: lower = better
        if use_cosine {
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        } else {
            scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        }

        let results: Vec<serde_json::Value> = scored.into_iter().take(count).map(|(score, meta)| {
            serde_json::json!({ "metadata": meta, "score": score })
        }).collect();

        serde_json::to_string(&results)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "query", e.to_string()))
    }
}

pub fn setup_vector_database(ctx: Ctx<'_>) -> rquickjs::Result<()> {
    rquickjs::Class::<VectorDatabase>::define(&ctx.globals())?;
    ctx.eval::<(), _>(r#"
VectorDatabase.prototype.addItem = function(embedding, metadata) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { self.__addItemSync(JSON.stringify(embedding), JSON.stringify(metadata)); resolve(); }
        catch(e) { reject(e); }
    });
};
VectorDatabase.prototype.query = function(embedding, count, metric) {
    var self = this;
    if (count === undefined) count = 10;
    if (metric === undefined) metric = 'cosine';
    return new Promise(function(resolve, reject) {
        try { resolve(JSON.parse(self.__querySync(JSON.stringify(embedding), count, metric))); }
        catch(e) { reject(e); }
    });
};
"#)?;
    Ok(())
}

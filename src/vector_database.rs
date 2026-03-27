use rquickjs::{class::Trace, Ctx, JsLifetime};
use std::cell::RefCell;
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

struct VectorItem {
    embedding: Vec<f32>,
    magnitude: f32,
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

// Wrapper for f32 that implements Ord via total_cmp (no extra dependencies)
#[derive(PartialEq)]
struct OrdF32(f32);

impl Eq for OrdF32 {}

impl PartialOrd for OrdF32 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrdF32 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

fn compute_magnitude(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

fn cosine_similarity(a: &[f32], a_mag: f32, b: &[f32], b_mag: f32) -> f32 {
    if a_mag == 0.0 || b_mag == 0.0 {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    dot / (a_mag * b_mag)
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
                let embedding: Vec<f32> = item.get("embedding")?
                    .as_array()?
                    .iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect();
                let magnitude = compute_magnitude(&embedding);
                let metadata = item.get("metadata")?.clone();
                Some(VectorItem { embedding, magnitude, metadata })
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
        let magnitude = compute_magnitude(&embedding);
        let metadata: serde_json::Value = serde_json::from_str(&metadata_json)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "addItem", e.to_string()))?;
        self.items.borrow_mut().push(VectorItem { embedding, magnitude, metadata });
        if let Some(ref path) = self.file_path {
            save_to_file(&self.items.borrow(), path)?;
        }
        Ok(())
    }

    #[qjs(rename = "__querySync")]
    pub fn query_sync(&self, embedding_json: String, count: usize, metric: String) -> rquickjs::Result<String> {
        let query = parse_embedding(&embedding_json)?;
        let query_mag = compute_magnitude(&query);
        let items = self.items.borrow();

        let use_cosine = matches!(metric.as_str(), "cosine" | "");
        let use_euclidean = matches!(metric.as_str(), "euclidean" | "euclidian");

        // Use a bounded heap for O(n log k) top-k selection instead of O(n log n) full sort.
        // For cosine (higher=better): min-heap of size k — evict the smallest when heap exceeds k.
        // For distance metrics (lower=better): max-heap of size k — evict the largest when heap exceeds k.
        let results: Vec<serde_json::Value> = if use_cosine {
            // min-heap: Reverse so BinaryHeap (max-heap) behaves as a min-heap
            let mut heap: BinaryHeap<(Reverse<OrdF32>, usize)> = BinaryHeap::with_capacity(count + 1);
            for (i, item) in items.iter().enumerate() {
                let score = cosine_similarity(&query, query_mag, &item.embedding, item.magnitude);
                heap.push((Reverse(OrdF32(score)), i));
                if heap.len() > count {
                    heap.pop(); // evict the smallest score
                }
            }
            // Drain heap and sort descending by score
            let mut top: Vec<(f32, usize)> = heap.into_iter().map(|(Reverse(OrdF32(s)), i)| (s, i)).collect();
            top.sort_by(|a, b| b.0.total_cmp(&a.0));
            top.into_iter().map(|(score, i)| {
                serde_json::json!({ "metadata": &items[i].metadata, "score": score })
            }).collect()
        } else {
            // max-heap for distance metrics (lower=better) — evict the largest distance
            let mut heap: BinaryHeap<(OrdF32, usize)> = BinaryHeap::with_capacity(count + 1);
            for (i, item) in items.iter().enumerate() {
                let score = if use_euclidean {
                    euclidean_distance(&query, &item.embedding)
                } else {
                    manhattan_distance(&query, &item.embedding)
                };
                heap.push((OrdF32(score), i));
                if heap.len() > count {
                    heap.pop(); // evict the largest distance
                }
            }
            // Drain heap and sort ascending by score
            let mut top: Vec<(f32, usize)> = heap.into_iter().map(|(OrdF32(s), i)| (s, i)).collect();
            top.sort_by(|a, b| a.0.total_cmp(&b.0));
            top.into_iter().map(|(score, i)| {
                serde_json::json!({ "metadata": &items[i].metadata, "score": score })
            }).collect()
        };

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

use std::path::Path;

const HF_BASE: &str = "https://huggingface.co";

struct ModelSpec {
    repo: &'static str,
    subdir: &'static str,
    files: &'static [&'static str],
}

const MODELS: &[ModelSpec] = &[
    ModelSpec {
        repo: "Qdrant/all-MiniLM-L6-v2-onnx",
        subdir: "minilm",
        files: &[
            "model.onnx",
            "tokenizer.json",
            "vocab.txt",
            "tokenizer_config.json",
            "special_tokens_map.json",
            "config.json",
        ],
    },
    ModelSpec {
        repo: "Xenova/distilbert-base-cased-distilled-squad",
        subdir: "qa",
        files: &[
            "onnx/model_int8.onnx",
            "tokenizer.json",
            "vocab.txt",
            "tokenizer_config.json",
        ],
    },
    ModelSpec {
        repo: "Xenova/distilbert-base-uncased-finetuned-sst-2-english",
        subdir: "sentiment",
        files: &[
            "onnx/model_int8.onnx",
            "tokenizer.json",
            "vocab.txt",
            "tokenizer_config.json",
            "config.json",
        ],
    },
    ModelSpec {
        repo: "Xenova/bert-base-NER",
        subdir: "ner",
        files: &[
            "onnx/model_quantized.onnx",
            "tokenizer.json",
            "vocab.txt",
            "tokenizer_config.json",
            "config.json",
        ],
    },
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let models_base = Path::new(&out_dir).join("models");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .expect("http client");

    for spec in MODELS {
        let dir = models_base.join(spec.subdir);
        std::fs::create_dir_all(&dir).unwrap();

        for file in spec.files {
            // Flatten subpath: "onnx/model_int8.onnx" -> stored as "model_int8.onnx"
            let local_name = Path::new(file).file_name().unwrap();
            let dest = dir.join(local_name);

            if dest.exists() {
                continue;
            }

            let url = format!("{}/{}/resolve/main/{}", HF_BASE, spec.repo, file);
            println!("cargo:warning=Downloading {} -> {} ...", url, dest.display());

            let resp = client
                .get(&url)
                .send()
                .unwrap_or_else(|e| panic!("GET {url}: {e}"));
            let bytes = resp
                .bytes()
                .unwrap_or_else(|e| panic!("reading {url}: {e}"));
            std::fs::write(&dest, &bytes)
                .unwrap_or_else(|e| panic!("writing {}: {e}", dest.display()));
        }
    }
}

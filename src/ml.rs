use directories::ProjectDirs;
use ort::session::{Session, builder::GraphOptimizationLevel};
use reqwest::Client;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use futures_util::StreamExt;
use tokenizers::Tokenizer;

pub struct MLEngine {
    session: Session,
    tokenizer: Tokenizer,
}

impl MLEngine {
    pub fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(model_path)?;

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("Tokenizer load error: {}", e))?;

        Ok(Self { session, tokenizer })
    }

    pub fn get_data_dir() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("com", "SemanticClipboard", "SemanticClipboard") {
            proj_dirs.data_local_dir().to_path_buf()
        } else {
            PathBuf::from(".")
        }
    }

    pub async fn download_models_if_needed<F>(progress_callback: F) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>>
    where
        F: Fn(f32) + Send + Sync + 'static,
    {
        let data_dir = Self::get_data_dir();
        fs::create_dir_all(&data_dir).await?;

        let model_path = data_dir.join("model.onnx");
        let tokenizer_path = data_dir.join("tokenizer.json");

        let model_url = "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx";
        let tokenizer_url = "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json";

        let client = Client::new();

        if !tokenizer_path.exists() {
            println!("Downloading tokenizer...");
            let res = client.get(tokenizer_url).send().await?;
            let bytes = res.bytes().await?;
            fs::write(&tokenizer_path, bytes).await?;
        }

        if !model_path.exists() {
            println!("Downloading model...");
            let res = client.get(model_url).send().await?;
            let total_size = res.content_length().unwrap_or(0) as f32;
            
            let mut file = fs::File::create(&model_path).await?;
            let mut stream = res.bytes_stream();
            let mut downloaded: f32 = 0.0;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as f32;
                if total_size > 0.0 {
                    progress_callback(downloaded / total_size);
                }
            }
        }

        Ok((model_path, tokenizer_path))
    }

    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        // Tokenize
        let encoding = self.tokenizer.encode(text, true)
            .map_err(|e| format!("Encoding error: {}", e))?;
        
        let ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&x| x as i64).collect();
        let type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&x| x as i64).collect();
        
        let batch_size = 1;
        let seq_len = ids.len();

        let ids_array = ndarray::Array2::from_shape_vec((batch_size, seq_len), ids)?;
        let mask_array = ndarray::Array2::from_shape_vec((batch_size, seq_len), attention_mask)?;
        let type_ids_array = ndarray::Array2::from_shape_vec((batch_size, seq_len), type_ids)?;

        let ids_tensor = ort::value::Tensor::from_array(ids_array)?;
        let mask_tensor = ort::value::Tensor::from_array(mask_array)?;
        let type_ids_tensor = ort::value::Tensor::from_array(type_ids_array)?;

        let inputs = ort::inputs![
            "input_ids" => ids_tensor,
            "attention_mask" => mask_tensor,
            "token_type_ids" => type_ids_tensor
        ];

        let outputs = self.session.run(inputs)?;
        
        // all-MiniLM-L6-v2 outputs: last_hidden_state. We typically mean pool it.
        // For simplicity, if we don't have a pooler, we can just take the first token (CLS token).
        let (_shape, data) = outputs["last_hidden_state"].try_extract_tensor::<f32>()?;
        
        // Shape is [batch=1, seq_len, 384]. CLS is at index 0.
        let mut embedding = Vec::with_capacity(384);
        for i in 0..384 {
            embedding.push(data[i]);
        }
        
        // L2 normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in embedding.iter_mut() {
                *x /= norm;
            }
        }

        Ok(embedding)
    }

    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }
        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;
        for i in 0..a.len() {
            dot += a[i] * b[i];
            norm_a += a[i] * a[i];
            norm_b += b[i] * b[i];
        }
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot / (norm_a.sqrt() * norm_b.sqrt())
        }
    }
}

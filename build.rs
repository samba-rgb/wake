use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Tell cargo to rerun this build script if the static commands file changes
    println!("cargo:rerun-if-changed=src/search/static_commands.json");
    
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("tfidf_index.rs");
    
    // Check if static commands file exists, if not create empty index
    let commands_path = "src/search/static_commands.json";
    if !Path::new(commands_path).exists() {
        // Create empty index file
        let empty_index = "
pub static TFIDF_INDEX: &[u8] = &[];
pub static COMMANDS_COUNT: usize = 0;
";
        fs::write(&dest_path, empty_index).unwrap();
        return;
    }
    
    // Read and parse the static commands JSON
    let json_content = fs::read_to_string(commands_path)
        .expect("Failed to read static_commands.json");
    
    let commands: Vec<serde_json::Value> = serde_json::from_str(&json_content)
        .expect("Failed to parse static_commands.json");
    
    // Build TF-IDF index
    let index = build_tfidf_index(&commands);
    
    // Serialize the index using JSON - simple and cross-platform
    let serialized = serde_json::to_vec(&index).unwrap();
    
    // Generate Rust code that includes the serialized index
    let generated_code = format!(
        "pub static TFIDF_INDEX: &[u8] = &{:?};\npub static COMMANDS_COUNT: usize = {};",
        serialized,
        commands.len()
    );
    
    fs::write(&dest_path, generated_code).unwrap();
    
    println!("Built TF-IDF index for {} commands", commands.len());
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TfIdfIndex {
    commands: Vec<StaticCommand>,
    vocabulary: HashMap<String, f64>, // word -> IDF score
    document_vectors: Vec<HashMap<String, f64>>, // TF-IDF vectors per document
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct StaticCommand {
    command: String,
    description: String,
}

fn build_tfidf_index(commands: &[serde_json::Value]) -> TfIdfIndex {
    let mut static_commands = Vec::new();
    let mut documents = Vec::new();
    
    // Parse commands and create searchable documents
    for cmd in commands {
        let command = cmd["command"].as_str().unwrap_or("").to_string();
        let description = cmd["description"].as_str().unwrap_or("").to_string();
        
        let static_cmd = StaticCommand { command: command.clone(), description: description.clone() };
        let document = format!("{command} {description}");
        
        static_commands.push(static_cmd);
        documents.push(document);
    }
    
    // Tokenize all documents
    let tokenized_docs: Vec<Vec<String>> = documents
        .iter()
        .map(|doc| tokenize(doc))
        .collect();
    
    // Build vocabulary and calculate document frequencies
    let mut vocabulary = HashMap::new();
    let total_docs = tokenized_docs.len() as f64;
    
    for tokens in &tokenized_docs {
        let unique_tokens: std::collections::HashSet<_> = tokens.iter().collect();
        for token in unique_tokens {
            *vocabulary.entry(token.clone()).or_insert(0) += 1;
        }
    }
    
    // Calculate IDF scores
    let idf_scores: HashMap<String, f64> = vocabulary
        .iter()
        .map(|(word, &doc_freq)| {
            let idf = (total_docs / doc_freq as f64).ln();
            (word.clone(), idf)
        })
        .collect();
    
    // Calculate TF-IDF vectors for each document
    let mut document_vectors = Vec::new();
    
    for tokens in &tokenized_docs {
        let mut tf_counts = HashMap::new();
        let total_terms = tokens.len() as f64;
        
        // Count term frequencies
        for token in tokens {
            *tf_counts.entry(token.clone()).or_insert(0) += 1;
        }
        
        // Calculate TF-IDF for each term
        let mut tfidf_vector = HashMap::new();
        for (term, &count) in &tf_counts {
            let tf = count as f64 / total_terms;
            let idf = idf_scores.get(term).unwrap_or(&0.0);
            let tfidf = tf * idf;
            
            if tfidf > 0.0 {
                tfidf_vector.insert(term.clone(), tfidf);
            }
        }
        
        document_vectors.push(tfidf_vector);
    }
    
    TfIdfIndex {
        commands: static_commands,
        vocabulary: idf_scores,
        document_vectors,
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let stop_words: std::collections::HashSet<&str> = [
        "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with", "by",
        "from", "is", "are", "was", "were", "be", "been", "have", "has", "had", "do", "does", "did",
        "will", "would", "could", "should", "may", "might", "can", "this", "that", "these", "those",
    ].iter().cloned().collect();
    
    text.to_lowercase()
        .split_whitespace()
        .map(|word| {
            // Remove punctuation and keep only alphanumeric characters
            word.chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect::<String>()
        })
        .filter(|word| !word.is_empty() && !stop_words.contains(word.as_str()) && word.len() > 1)
        .collect()
}
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use bincode;

// Include the generated TF-IDF index from build script
include!(concat!(env!("OUT_DIR"), "/tfidf_index.rs"));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticCommand {
    pub command: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TfIdfIndex {
    commands: Vec<StaticCommand>,
    vocabulary: HashMap<String, f64>, // word -> IDF score
    document_vectors: Vec<HashMap<String, f64>>, // TF-IDF vectors per document
}

pub struct TfIdfSearcher {
    index: TfIdfIndex,
}

impl TfIdfSearcher {
    pub fn new() -> Result<Self> {
        if TFIDF_INDEX.is_empty() {
            anyhow::bail!("No TF-IDF index available. Static commands file not found during build.");
        }
        
        let index: TfIdfIndex = bincode::deserialize(TFIDF_INDEX)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize TF-IDF index: {}", e))?;
        
        Ok(TfIdfSearcher { index })
    }
    
    pub fn search(&self, query: &str) -> Option<&StaticCommand> {
        if query.trim().is_empty() {
            return None;
        }
        
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return None;
        }
        
        // First try exact TF-IDF matching
        if let Some(result) = self.tfidf_search(&query_tokens) {
            return Some(result);
        }
        
        // If no exact match, try fuzzy matching for partial terms
        self.fuzzy_search(query)
    }
    
    fn tfidf_search(&self, query_tokens: &[String]) -> Option<&StaticCommand> {
        // Calculate query vector
        let mut query_vector = HashMap::new();
        let query_len = query_tokens.len() as f64;
        
        // Count term frequencies in query
        let mut tf_counts = HashMap::new();
        for token in query_tokens {
            *tf_counts.entry(token.clone()).or_insert(0) += 1;
        }
        
        // Calculate TF-IDF for query terms
        for (term, &count) in &tf_counts {
            let tf = count as f64 / query_len;
            let idf = self.index.vocabulary.get(term).unwrap_or(&0.0);
            let tfidf = tf * idf;
            
            if tfidf > 0.0 {
                query_vector.insert(term.clone(), tfidf);
            }
        }
        
        if query_vector.is_empty() {
            return None;
        }
        
        // Calculate cosine similarity with each document
        let mut best_score = 0.0;
        let mut best_index = None;
        
        for (doc_idx, doc_vector) in self.index.document_vectors.iter().enumerate() {
            let similarity = cosine_similarity(&query_vector, doc_vector);
            
            if similarity > best_score {
                best_score = similarity;
                best_index = Some(doc_idx);
            }
        }
        
        // Return best match if similarity is above threshold
        if best_score > 0.1 {
            best_index.map(|idx| &self.index.commands[idx])
        } else {
            None
        }
    }
    
    fn fuzzy_search(&self, query: &str) -> Option<&StaticCommand> {
        let query_lower = query.to_lowercase();
        let mut best_score = 0.0;
        let mut best_match = None;
        
        for command in &self.index.commands {
            let combined_text = format!("{} {}", command.command, command.description).to_lowercase();
            
            // Check for substring matches
            let mut score = 0.0;
            
            // Exact substring match gets highest score
            if combined_text.contains(&query_lower) {
                score += 1.0;
            }
            
            // Check for partial matches (e.g., "threaddump" matches "thread-dump")
            let normalized_query = query_lower.replace("-", "").replace("_", "");
            let normalized_text = combined_text.replace("-", "").replace("_", "");
            
            if normalized_text.contains(&normalized_query) {
                score += 0.8;
            }
            
            // Check for word boundary matches
            for word in query_lower.split_whitespace() {
                if combined_text.contains(word) {
                    score += 0.5;
                }
            }
            
            if score > best_score {
                best_score = score;
                best_match = Some(command);
            }
        }
        
        // Return match if score is above threshold
        if best_score > 0.3 {
            best_match
        } else {
            None
        }
    }
    
    pub fn get_all_commands(&self) -> &[StaticCommand] {
        &self.index.commands
    }
    
    pub fn commands_count(&self) -> usize {
        self.index.commands.len()
    }
}

fn cosine_similarity(vec1: &HashMap<String, f64>, vec2: &HashMap<String, f64>) -> f64 {
    let mut dot_product = 0.0;
    let mut norm1 = 0.0;
    let mut norm2 = 0.0;
    
    // Calculate dot product and norms
    for (term, &val1) in vec1 {
        dot_product += val1 * vec2.get(term).unwrap_or(&0.0);
        norm1 += val1 * val1;
    }
    
    for &val2 in vec2.values() {
        norm2 += val2 * val2;
    }
    
    if norm1 == 0.0 || norm2 == 0.0 {
        return 0.0;
    }
    
    dot_product / (norm1.sqrt() * norm2.sqrt())
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
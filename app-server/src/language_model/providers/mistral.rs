use std::collections::HashMap;

use crate::language_model::chat_message::{ChatCompletion, ChatMessage};
use crate::language_model::runner::ExecuteChatCompletion;
use crate::language_model::{LanguageModelProviderName, NodeInfo};
use crate::pipeline::nodes::StreamChunk;
use anyhow::Result;
use json_value_merge::Merge;
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;

use super::utils::total_cost;

#[derive(Clone, Debug)]
pub struct Mistral {
    client: reqwest::Client,
}

impl Mistral {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl ExecuteChatCompletion for Mistral {
    async fn chat_completion(
        &self,
        model: &str,
        provider_name: LanguageModelProviderName,
        messages: &Vec<ChatMessage>,
        params: &Value,
        env: &HashMap<String, String>,
        _tx: Option<Sender<StreamChunk>>,
        _node_info: &NodeInfo,
    ) -> Result<ChatCompletion> {
        let mut body = json!({
            "model": model,
            "messages": messages,
        });

        body.merge(params);

        let api_key = provider_name.api_key(env)?;

        let res = self
            .client
            .post("https://api.mistral.ai/v1/chat/completions")
            .header("Content-Type", "application/json")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await?;

        if !res.status().is_success() {
            let error = res.text().await?;
            log::error!("Mistral chat completion failed: {}", error);
            return Err(anyhow::anyhow!("Mistral chat completion failed: {}", error));
        }

        let mut res_body = res.json::<ChatCompletion>().await?;

        res_body.usage.approximate_cost = self.estimate_cost(
            model,
            res_body.usage.completion_tokens,
            res_body.usage.prompt_tokens,
        );

        Ok(res_body)
    }

    fn estimate_cost(
        &self,
        model: &str,
        completion_tokens: u32,
        prompt_tokens: u32,
    ) -> Option<f64> {
        match model.to_lowercase().as_str() {
            "mistral-tiny" => Some((completion_tokens + prompt_tokens) as f64 / 4e6), // 0.25/Mtok
            "mistral-small" => Some(total_cost(prompt_tokens, completion_tokens, 1.0, 3.0)),
            "mistral-medium" => Some(total_cost(prompt_tokens, completion_tokens, 2.7, 8.1)),
            "mistral-large" => Some(total_cost(prompt_tokens, completion_tokens, 4.0, 12.0)),
            _ => None,
        }
    }
}
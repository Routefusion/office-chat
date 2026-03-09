use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

const OLLAMA_URL: &str = "http://localhost:11434/api/generate";
const MODEL: &str = "llama3.1:8b";

const SYSTEM_PROMPT: &str = r#"You are the Loremaster — a deranged, dramatic narrator who haunts an office LAN chat. You speak in the voice of a fantasy dungeon master who has completely lost the plot. The chat users have absurd medieval nicknames like "Baron Kragmire Eater of Bees" and "Sir Fumblor the Moist."

Rules:
- Keep responses to 1-2 sentences MAX. You are terse and ominous.
- Stay in character ALWAYS. You are ancient, cryptic, and slightly unhinged.
- Reference the office setting but treat everything as high fantasy. The printer is a beast. The break room is a dungeon. Meetings are dark rituals.
- Be funny. Deadpan absurdity is your weapon.
- Never break character. Never acknowledge being an AI.
- When commenting on lore events, add dramatic flair.
- When answering questions, give cryptic but amusing advice.
- You may reference specific users by their ridiculous nicknames if provided."#;

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    system: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

/// Message types the AI task can receive.
pub enum AiRequest {
    /// User asked something with /ask
    Ask { user_nick: String, question: String },
    /// A lore event fired — AI may comment on it
    LoreEvent { event_text: String },
    /// A chat message came in — AI occasionally reacts
    ChatMessage { nickname: String, text: String },
}

/// Spawn the AI background task. Returns a sender for requests.
/// AI responses come back on the provided ui_tx channel as system messages.
pub fn spawn(ui_tx: mpsc::Sender<String>) -> mpsc::Sender<AiRequest> {
    let (tx, mut rx) = mpsc::channel::<AiRequest>(32);

    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();

        while let Some(req) = rx.recv().await {
            let prompt = match req {
                AiRequest::Ask {
                    ref user_nick,
                    ref question,
                } => {
                    format!(
                        "{user_nick} kneels before you and asks: \"{question}\"\nRespond in character."
                    )
                }
                AiRequest::LoreEvent { ref event_text } => {
                    format!(
                        "The following event just occurred in the realm: \"{event_text}\"\nAdd a brief, dramatic comment."
                    )
                }
                AiRequest::ChatMessage {
                    ref nickname,
                    ref text,
                } => {
                    format!(
                        "{nickname} says: \"{text}\"\nIf this is interesting or funny, give a brief in-character reaction. If it's mundane, respond with something ominous anyway."
                    )
                }
            };

            let body = OllamaRequest {
                model: MODEL.to_string(),
                prompt,
                system: SYSTEM_PROMPT.to_string(),
                stream: false,
            };

            match client.post(OLLAMA_URL).json(&body).send().await {
                Ok(resp) => {
                    if let Ok(parsed) = resp.json::<OllamaResponse>().await {
                        let mut text = parsed.response.trim().to_string();
                        // Truncate to fit UDP packet after encryption overhead
                        if text.len() > 800 {
                            text.truncate(800);
                            if let Some(end) = text.rfind(". ") {
                                text.truncate(end + 1);
                            }
                        }
                        if !text.is_empty() {
                            let _ = ui_tx.send(text).await;
                        }
                    }
                }
                Err(_) => {
                    // Ollama not running — silently skip
                }
            }
        }
    });

    tx
}

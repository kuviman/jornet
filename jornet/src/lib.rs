mod http;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

/// Used to interact with Jornet leaderboard.
pub struct Leaderboard {
    id: Uuid,
    key: Uuid,
    host: String,
    player: Option<Player>,
}

impl Leaderboard {
    pub fn with_host_and_leaderboard(host: Option<String>, id: Uuid, key: Uuid) -> Self {
        Self {
            id,
            key,
            host: host.unwrap_or_else(|| "https://jornet.vleue.com".to_string()),
            player: Default::default(),
        }
    }

    /// Get the current player.
    ///
    /// This can be used to get the random name generated if one was not specified when
    /// creating the player, or to save the `id`/`key` locally to be able to reconnect later
    /// as the same player.
    pub fn get_player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    /// Create a player. If you don't specify a name, one will be generated randomly.
    ///
    /// Either this or [`Self::as_player`] must be called before sending a score.
    pub async fn create_player(&mut self, name: Option<&str>) -> anyhow::Result<&Player> {
        let player = PlayerInput {
            name: name.map(|n| n.to_string()),
        };
        if let Some(player) = http::post(&format!("{}/api/v1/players", self.host), player).await {
            self.player = Some(player);
            Ok(self.player.as_ref().unwrap())
        } else {
            anyhow::bail!("error creating a player");
        }
    }

    /// Connect as a returning player.
    ///
    /// Either this or [`Self::create_player`] must be called before sending a score.
    pub fn as_player(&mut self, player: Player) {
        self.player = Some(player);
    }

    /// Send a score to the leaderboard.
    pub async fn send_score(&self, score: f32) -> Option<()> {
        self.inner_send_score_with_meta(score, None).await
    }

    /// Send a score with metadata to the leaderboard.
    ///
    /// Metadata can be information about the game, victory conditions, ...
    pub async fn send_score_with_meta(&self, score: f32, meta: &str) -> Option<()> {
        self.inner_send_score_with_meta(score, Some(meta.to_string()))
            .await
    }

    async fn inner_send_score_with_meta(&self, score: f32, meta: Option<String>) -> Option<()> {
        let leaderboard_id = self.id;
        let host = self.host.clone();

        if let Some(player) = self.player.as_ref() {
            let score_to_send = ScoreInput::new(self.key, score, player, meta);
            if http::post::<_, ()>(
                &format!("{}/api/v1/scores/{}", host, leaderboard_id),
                score_to_send,
            )
            .await
            .is_none()
            {
                return None; // TODO warn!("error sending the score");
            }
            Some(())
        } else {
            None
        }
    }

    /// Get the leaderboard data.
    pub async fn get_leaderboard(&self) -> anyhow::Result<Vec<Score>> {
        if let Some(scores) = http::get(&format!("{}/api/v1/scores/{}", self.host, self.id)).await {
            Ok(scores)
        } else {
            anyhow::bail!("error getting the leaderboard")
        }
    }
}

/// A score from a leaderboard
#[derive(Deserialize, Debug, Clone)]
pub struct Score {
    /// The score.
    pub score: f32,
    /// The player name.
    pub player: String,
    /// Optional metadata.
    pub meta: Option<String>,
    /// Timestamp of the score.
    pub timestamp: String,
}

#[derive(Serialize)]
struct ScoreInput {
    pub score: f32,
    pub player: Uuid,
    pub meta: Option<String>,
    pub timestamp: u64,
    pub k: String,
}

impl ScoreInput {
    fn new(leaderboard_key: Uuid, score: f32, player: &Player, meta: Option<String>) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        #[cfg(target_arch = "wasm32")]
        let timestamp = (js_sys::Date::now() / 1000.0) as u64;

        let mut mac = Hmac::<Sha256>::new_from_slice(player.key.as_bytes()).unwrap();
        mac.update(&timestamp.to_le_bytes());
        mac.update(leaderboard_key.as_bytes());
        mac.update(player.id.as_bytes());
        mac.update(&score.to_le_bytes());
        if let Some(meta) = meta.as_ref() {
            mac.update(meta.as_bytes());
        }

        let hmac = hex::encode(&mac.finalize().into_bytes()[..]);
        Self {
            score,
            player: player.id,
            meta,
            timestamp,
            k: hmac,
        }
    }
}

/// A player, as returned from the server
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Player {
    /// its ID
    pub id: Uuid,
    /// its key, this should be kept secret
    pub key: Uuid,
    /// its name, changing it here won't be reflected on the server
    pub name: String,
}

#[derive(Serialize, Debug, Clone)]
struct PlayerInput {
    name: Option<String>,
}

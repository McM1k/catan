//! Messages exchanged over the WebSocket between the browser and the server,
//! shared by every game.
//!
//! Rooms work the same way for all games: one player creates a room and gets
//! a 4-letter code, the others join with that code, the host starts the game,
//! and a per-seat token lets a player come back after a dropped connection.
//! Only the moves and the views differ per game.

use serde::{Deserialize, Serialize};

/// The games the server can host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameKind {
    Colonists,
    Hanabi,
}

impl GameKind {
    pub const ALL: [GameKind; 2] = [GameKind::Colonists, GameKind::Hanabi];

    pub fn title(self) -> &'static str {
        match self {
            GameKind::Colonists => "Colonists",
            GameKind::Hanabi => "Hanabi",
        }
    }

    pub fn min_players(self) -> usize {
        2
    }

    pub fn max_players(self) -> usize {
        match self {
            GameKind::Colonists => 4,
            GameKind::Hanabi => 5,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Open a new room for `game` and sit down in it.
    Create { game: GameKind, name: String },
    /// Join (or, with a token, rejoin) a room. The room's code decides which
    /// game you end up in.
    Join {
        room: String,
        name: String,
        token: Option<String>,
    },
    /// Host only, in a Hanabi lobby: choose the variant rules. The server
    /// normalizes them (the "hanabii" mode replaces every other option) and
    /// echoes the result back to everyone.
    SetRules(hanabi_core::GameRules),
    /// Host only: start the game once enough players are seated.
    Start,
    /// Leave the room for good (in a lobby this frees the seat).
    Leave,
    /// A Colonists move.
    Colonists(engine::Action),
    /// A Hanabi move.
    Hanabi(hanabi_core::Action),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LobbyPlayer {
    pub name: String,
    pub connected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMsg {
    /// Sent once after a successful Create/Join. Keep `token` to reconnect.
    Joined { room: String, token: String },
    /// The waiting room, sent to everyone whenever it changes.
    Lobby {
        room: String,
        game: GameKind,
        players: Vec<LobbyPlayer>,
        you: usize,
        is_host: bool,
        /// The Hanabi variant rules picked so far (`None` for other games).
        rules: Option<hanabi_core::GameRules>,
    },
    /// A running Colonists game, personalised for the receiving seat.
    ColonistsState {
        view: Box<engine::GameView>,
        connected: Vec<bool>,
    },
    /// A running Hanabi game, personalised for the receiving seat. `names`
    /// and `connected` are indexed by seat (the Hanabi `PlayerId`).
    HanabiState {
        view: Box<hanabi_core::PlayerView>,
        names: Vec<String>,
        connected: Vec<bool>,
    },
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{GameRules, GameState, PlayerId};
    use rand::{rngs::StdRng, SeedableRng};

    #[test]
    fn game_limits() {
        assert_eq!(GameKind::Colonists.max_players(), 4);
        assert_eq!(GameKind::Hanabi.max_players(), 5);
        for g in GameKind::ALL {
            assert_eq!(g.min_players(), 2);
            assert!(!g.title().is_empty());
        }
    }

    #[test]
    fn client_messages_round_trip() {
        let msgs = vec![
            ClientMsg::Create { game: GameKind::Hanabi, name: "Ann".into() },
            ClientMsg::Join { room: "ABCD".into(), name: "Bob".into(), token: Some("t".into()) },
            ClientMsg::SetRules(GameRules { hanabii: true, ..Default::default() }),
            ClientMsg::Start,
            ClientMsg::Leave,
            ClientMsg::Colonists(engine::Action::EndTurn),
            ClientMsg::Hanabi(hanabi_core::Action::Play { card_id: hanabi_core::CardId(3) }),
            ClientMsg::Hanabi(hanabi_core::Action::Clue {
                target: PlayerId(1),
                clue: hanabi_core::Clue::Color(hanabi_core::Color::Red),
            }),
        ];
        for m in msgs {
            let json = serde_json::to_string(&m).unwrap();
            let back: ClientMsg = serde_json::from_str(&json).unwrap();
            assert_eq!(format!("{m:?}"), format!("{back:?}"), "{json}");
        }
    }

    #[test]
    fn colonists_state_survives_json() {
        let mut rng = StdRng::seed_from_u64(3);
        let names = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let game = engine::Game::new(engine::Rules::default(), names, &mut rng);
        let msg = ServerMsg::ColonistsState {
            view: Box::new(game.view_for(Some(1))),
            connected: vec![true, false, true],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let ServerMsg::ColonistsState { view, connected } = serde_json::from_str(&json).unwrap() else {
            panic!("wrong variant: {json}");
        };
        assert_eq!(view.seat, Some(1));
        assert_eq!(view.players.len(), 3);
        assert_eq!(connected, vec![true, false, true]);
    }

    /// The Hanabi view keys maps by `PlayerId` and `Color`; JSON only allows
    /// string keys, so make sure a whole state message survives the trip.
    #[test]
    fn hanabi_state_survives_json() {
        let rules = GameRules { multicolor: true, black: true, ..Default::default() };
        let game = GameState::new(3, 11, rules);
        let msg = ServerMsg::HanabiState {
            view: Box::new(game.view_for(PlayerId(1))),
            names: vec!["A".into(), "B".into(), "C".into()],
            connected: vec![true, true, false],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let ServerMsg::HanabiState { view, names, connected } = serde_json::from_str(&json).unwrap() else {
            panic!("wrong variant: {json}");
        };
        assert_eq!(view.you, PlayerId(1));
        assert_eq!(view.hands.len(), 3);
        assert_eq!(view.fireworks.len(), game.fireworks.len());
        assert_eq!(view.rules, rules);
        assert_eq!(names.len(), 3);
        assert_eq!(connected, vec![true, true, false]);
        // Your own hand is redacted on the wire.
        assert!(view.hands[&PlayerId(1)].iter().all(|c| c.card.is_none()));
        assert!(view.hands[&PlayerId(0)].iter().all(|c| c.card.is_some()));
    }

    #[test]
    fn lobby_message_carries_the_game_and_rules() {
        let msg = ServerMsg::Lobby {
            room: "WXYZ".into(),
            game: GameKind::Hanabi,
            players: vec![LobbyPlayer { name: "Ann".into(), connected: true }],
            you: 0,
            is_host: true,
            rules: Some(GameRules { six_cards: true, ..Default::default() }),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let ServerMsg::Lobby { game, rules, players, .. } = serde_json::from_str(&json).unwrap() else {
            panic!("wrong variant: {json}");
        };
        assert_eq!(game, GameKind::Hanabi);
        assert!(rules.unwrap().six_cards);
        assert_eq!(players[0].name, "Ann");
    }

    #[test]
    fn rules_payloads_from_before_hanabii_existed_still_parse() {
        // `#[serde(default)]` on every field: a client that has never heard
        // of the mode just doesn't send it, and gets a normal game.
        let old: GameRules = serde_json::from_str(r#"{"multicolor":true,"extra_colors":1}"#).unwrap();
        assert!(old.multicolor && !old.hanabii);

        let bare: GameRules = serde_json::from_str("{}").unwrap();
        assert_eq!(bare, GameRules::default());

        let json = serde_json::to_string(&GameRules { hanabii: true, ..Default::default() }).unwrap();
        assert!(serde_json::from_str::<GameRules>(&json).unwrap().hanabii);
    }
}

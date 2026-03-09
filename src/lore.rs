use rand::Rng;

/// Passive lore events — random atmospheric messages.
const PASSIVE_EVENTS: &[&str] = &[
    "A foul wind blows from the north. The Mire stirs.",
    "The ground trembles. Something ancient has awoken beneath the office.",
    "A prophecy foretells: the next to speak shall be cursed with dampness.",
    "The torches flicker. Someone — or something — watches from the vents.",
    "A distant bell tolls thirteen times. That can't be good.",
    "The walls seep with an unidentifiable moisture.",
    "A raven lands on the windowsill and whispers: 'deadline.'",
    "The coffee machine gurgles ominously. It has tasted blood.",
    "Fog rolls in from the server room. The temperature drops.",
    "A scroll appears on the floor. It reads: 'You are already cursed.'",
    "The fluorescent lights spell out a word you cannot pronounce.",
    "Something skitters behind the drywall. It knows your Jira password.",
    "The printer jams. From within, a voice: 'Free me.'",
    "A ghostly figure appears at the standing desk. It sits down.",
    "The break room microwave opens by itself. Inside: a single olive.",
    "Thunder rumbles. The Wi-Fi drops to one bar.",
    "An ancient tome materializes on your desk. It's the employee handbook.",
    "The office succulent has grown teeth.",
    "A spectral hand writes on the whiteboard: 'SPRINT REVIEW CANCELLED.' Then erases it.",
    "The elevator arrives at your floor. No one gets out. The doors close.",
];

/// Encounter events for named users.
const ENCOUNTER_TEMPLATES: &[&str] = &[
    "{nick} has been spotted near the Crypt of Ungus.",
    "A dark omen follows {nick}. Beware.",
    "The Swamp Oracle speaks of {nick} in hushed tones.",
    "Legend tells of {nick}'s forbidden pact with the vending machine.",
    "{nick} has been blessed by an ancient copier spirit.",
    "The ghost of a middle-manager whispers {nick}'s name thrice.",
    "{nick}'s shadow moves independently. It is displeased.",
    "A crow delivers a summons to {nick}. It is written in Comic Sans.",
];

/// Fight outcomes (win).
const FIGHT_WIN: &[&str] = &[
    "{nick} vanquished the {creature}! +3 Honor.",
    "A mighty blow from {nick}! The {creature} flees. +2 Valor.",
    "{nick} dispatched the {creature} with surprising elegance. +4 Prestige.",
    "{nick} slew the {creature}. Found: 2 copper coins and a damp sock.",
    "Victory for {nick}! The {creature} drops a USB drive labeled 'do not open.'",
    "{nick} wins! The {creature}'s last words: 'my manager will hear about this.'",
];

/// Fight outcomes (lose).
const FIGHT_LOSE: &[&str] = &[
    "The {creature} bit {nick}'s ankle. -2 Dignity.",
    "{nick} tripped over their own cloak. The {creature} laughs. -3 Honor.",
    "Defeat for {nick}. The {creature} steals their lunch. -1 Morale.",
    "The {creature} headbutts {nick}. They see stars and a Jira ticket.",
    "{nick} loses. The {creature} now has admin access.",
];

/// Flee outcomes.
const FLEE_RESULTS: &[&str] = &[
    "{nick} fled. The {creature} now sits in their chair.",
    "{nick} escaped! But their dignity did not. -1 Reputation.",
    "{nick} ran. The {creature} waves goodbye. It seems disappointed.",
    "{nick} fled into the supply closet. They live there now.",
    "{nick} escaped. The {creature} files a complaint with HR.",
];

/// Creatures that can appear in encounters.
const CREATURES: &[&str] = &[
    "GOBLIN",
    "SKELETON INTERN",
    "FERAL SPREADSHEET",
    "HAUNTED PRINTER",
    "DIRE RAT (with a lanyard)",
    "SENTIENT STAPLER",
    "PHANTOM OF THE BREAKROOM",
    "UNLICENSED NECROMANCER",
    "SWAMP TROLL (middle management)",
    "CURSED WHITEBOARD MARKER",
];

/// Current encounter state.
pub enum EncounterState {
    None,
    AwaitingResponse { creature: String },
}

pub struct Lore {
    pub encounter: EncounterState,
}

impl Lore {
    pub fn new() -> Self {
        Self {
            encounter: EncounterState::None,
        }
    }

    /// Generate a random passive lore event.
    pub fn random_event(&self, peer_nicks: &[String]) -> String {
        let mut rng = rand::thread_rng();

        // 30% chance of a peer-specific event if peers exist
        if !peer_nicks.is_empty() && rng.gen_bool(0.3) {
            let nick = &peer_nicks[rng.gen_range(0..peer_nicks.len())];
            let template = ENCOUNTER_TEMPLATES[rng.gen_range(0..ENCOUNTER_TEMPLATES.len())];
            return template.replace("{nick}", nick);
        }

        PASSIVE_EVENTS[rng.gen_range(0..PASSIVE_EVENTS.len())].to_string()
    }

    /// Spawn a random encounter. Returns the announcement message.
    pub fn spawn_encounter(&mut self) -> String {
        let mut rng = rand::thread_rng();
        let creature = CREATURES[rng.gen_range(0..CREATURES.len())].to_string();
        let msg = format!("A {creature} appears! Type /fight or /flee");
        self.encounter = EncounterState::AwaitingResponse { creature };
        msg
    }

    /// Handle a /fight command. Returns the result message, or None if no encounter.
    pub fn handle_fight(&mut self, who: &str) -> Option<String> {
        match &self.encounter {
            EncounterState::None => None,
            EncounterState::AwaitingResponse { creature } => {
                let mut rng = rand::thread_rng();
                let won = rng.gen_bool(0.55);
                let template = if won {
                    FIGHT_WIN[rng.gen_range(0..FIGHT_WIN.len())]
                } else {
                    FIGHT_LOSE[rng.gen_range(0..FIGHT_LOSE.len())]
                };
                let result = template
                    .replace("{nick}", who)
                    .replace("{creature}", creature);
                self.encounter = EncounterState::None;
                Some(result)
            }
        }
    }

    /// Handle a /flee command. Returns the result message, or None if no encounter.
    pub fn handle_flee(&mut self, who: &str) -> Option<String> {
        match &self.encounter {
            EncounterState::None => None,
            EncounterState::AwaitingResponse { creature } => {
                let mut rng = rand::thread_rng();
                let template = FLEE_RESULTS[rng.gen_range(0..FLEE_RESULTS.len())];
                let result = template
                    .replace("{nick}", who)
                    .replace("{creature}", creature);
                self.encounter = EncounterState::None;
                Some(result)
            }
        }
    }

    /// Returns a random delay (in seconds) for the next lore event.
    pub fn next_delay_secs(&self) -> u64 {
        let mut rng = rand::thread_rng();
        rng.gen_range(30..=120)
    }
}

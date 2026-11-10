pub const STEP_SECONDS: i64 = 10 * 60;
const MAX_OFFLINE_STEPS: i64 = 12 * 3600 / STEP_SECONDS;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pet {
    pub name: String,
    pub born: i64,
    pub last_tick: i64,
    pub fullness: i64,
    pub happiness: i64,
    pub energy: i64,
    pub hygiene: i64,
    pub health: i64,
    pub sleeping: bool,
    pub last_action: i64,
    #[serde(default)]
    pub care_points: i64,
    #[serde(default)]
    pub died_at: Option<i64>,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Feed,
    Play,
    Clean,
    Sleep,
    Medicine,
}

impl Pet {
    pub fn new(name: String, now: i64) -> Self {
        Self {
            name,
            born: now,
            last_tick: now,
            fullness: 80,
            happiness: 80,
            energy: 80,
            hygiene: 80,
            health: 100,
            sleeping: false,
            last_action: 0,
            care_points: 0,
            died_at: None,
        }
    }

    pub fn alive(&self) -> bool {
        self.health > 0
    }

    pub fn age(&self, now: i64) -> i64 {
        let until = if self.alive() {
            now
        } else {
            self.died_at.unwrap_or(self.last_tick).min(now)
        };
        until.saturating_sub(self.born).max(0)
    }

    pub fn stage(&self, now: i64) -> &'static str {
        match self.age(now) / 3600 {
            0 => "Baby",
            1..=5 => "Child",
            6..=23 => "Teen",
            _ => "Adult",
        }
    }

    pub fn bond_level(&self) -> i64 {
        1 + self.care_points / 40
    }

    pub fn progression(&self, now: i64) -> Progression {
        let age = self.age(now);
        let (start, end, next) = match age {
            0..3600 => (0, 3600, Some("Child")),
            3600..21600 => (3600, 21600, Some("Teen")),
            21600..86400 => (21600, 86400, Some("Adult")),
            _ => (86400, 86400, None),
        };
        Progression {
            stage: self.stage(now).into(),
            next_stage: next.map(str::to_owned),
            seconds_until_growth: if next.is_some() && self.alive() {
                Some(end - age)
            } else {
                None
            },
            growth_percent: if next.is_none() {
                100
            } else {
                ((age - start) * 100 / (end - start)).clamp(0, 100)
            },
            bond_level: self.bond_level(),
            bond_progress: self.care_points % 40,
            care_points: self.care_points,
            achievements: vec![
                Achievement {
                    title: "First little care".into(),
                    description: "Share your first helpful care action.".into(),
                    unlocked: self.care_points > 0,
                },
                Achievement {
                    title: "Good friends".into(),
                    description: "Reach bond level 2 together.".into(),
                    unlocked: self.bond_level() >= 2,
                },
                Achievement {
                    title: "All grown up".into(),
                    description: "Raise your buddy to adulthood.".into(),
                    unlocked: age >= 86400,
                },
            ],
        }
    }

    pub fn mood(&self) -> &'static str {
        if !self.alive() {
            "Resting in the stars"
        } else if self.sleeping {
            "Dreaming"
        } else if self.health < 40 {
            "Feeling poorly"
        } else if self.fullness < 25 {
            "Hungry"
        } else if self.energy < 25 {
            "Sleepy"
        } else if self.hygiene < 25 {
            "Needs a bath"
        } else if self.happiness < 30 {
            "Lonely"
        } else {
            "Happy to see you!"
        }
    }

    /// One simulation step per ten real minutes. Keep partial steps across saves.
    /// Long absences simulate at most twelve hours, then resume at the present.
    pub fn advance(&mut self, now: i64) -> i64 {
        if !self.alive() && self.died_at.is_none() {
            self.died_at = Some(self.last_tick);
        }
        let elapsed = now.saturating_sub(self.last_tick).max(0);
        let steps = (elapsed / STEP_SECONDS).min(MAX_OFFLINE_STEPS);
        for step in 0..steps {
            if !self.alive() {
                break;
            }
            self.fullness = (self.fullness - if self.sleeping { 1 } else { 2 }).max(0);
            self.happiness = (self.happiness - 1).max(0);
            self.hygiene = (self.hygiene - 1).max(0);
            self.energy = if self.sleeping {
                (self.energy + 6).min(100)
            } else {
                (self.energy - 1).max(0)
            };
            let damage = [self.fullness, self.hygiene, self.energy]
                .iter()
                .filter(|&&stat| stat == 0)
                .count() as i64
                * 3;
            self.health = (self.health - damage).max(0);
            if !self.alive() {
                self.died_at = Some(self.last_tick + (step + 1) * STEP_SECONDS);
            }
            if damage == 0 && self.fullness >= 60 && self.hygiene >= 60 {
                self.health = (self.health + 1).min(100);
            }
        }
        if steps > 0 {
            self.last_tick = now - elapsed % STEP_SECONDS;
        }
        steps
    }

    pub fn act(&mut self, action: Action, now: i64) -> String {
        if !self.alive() {
            return "Your buddy has passed away. You can hatch a new egg.".into();
        }
        if matches!(action, Action::Sleep) {
            self.sleeping = !self.sleeping;
            return if self.sleeping {
                "Tucked in. Sweet dreams!"
            } else {
                "Good morning, little buddy!"
            }
            .into();
        }
        if self.sleeping {
            return "Your buddy is asleep. Wake them before offering care.".into();
        }
        if now.saturating_sub(self.last_action) < 5 {
            return "Give your buddy a moment! Care actions have a 5-second cooldown.".into();
        }
        let (message, points) = match action {
            Action::Feed => {
                if self.fullness >= 100 {
                    return "Already full! Save that snack for later.".into();
                }
                self.fullness = (self.fullness + 25).min(100);
                self.hygiene = (self.hygiene - 3).max(0);
                ("Yum! A little snack makes a big difference.", 4)
            }
            Action::Play => {
                if self.happiness >= 100 {
                    return "Already full of joy! Save the next game for later.".into();
                }
                if self.energy < 15 {
                    return "Too tired to play. A nap will help.".into();
                }
                self.happiness = (self.happiness + 25).min(100);
                self.energy -= 15;
                self.fullness = (self.fullness - 5).max(0);
                ("You played catch together. Best friends forever!", 6)
            }
            Action::Clean => {
                if self.hygiene >= 100 {
                    return "Already squeaky clean! No bath needed yet.".into();
                }
                self.hygiene = 100;
                ("Splish splash! Squeaky clean.", 4)
            }
            Action::Medicine => {
                if self.health >= 100 {
                    return "Your buddy is healthy. No medicine needed.".into();
                }
                self.health = (self.health + 25).min(100);
                self.happiness = (self.happiness - 5).max(0);
                ("A little medicine. Get well soon, buddy.", 4)
            }
            Action::Sleep => unreachable!(),
        };
        self.last_action = now;
        self.care_points += points;
        message.into()
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Progression {
    pub stage: String,
    pub next_stage: Option<String>,
    pub seconds_until_growth: Option<i64>,
    pub growth_percent: i64,
    pub bond_level: i64,
    pub bond_progress: i64,
    pub care_points: i64,
    pub achievements: Vec<Achievement>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Achievement {
    pub title: String,
    pub description: String,
    pub unlocked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_steps_survive_repeated_updates() {
        let mut p = Pet::new("Mochi".into(), 1000);
        p.advance(1300);
        assert_eq!(p.advance(1601), 1);
        assert_eq!(p.last_tick, 1600);
        assert_eq!(p.fullness, 78);
        assert_eq!(p.advance(1590), 0);
    }
    #[test]
    fn sleeping_recovers_energy_but_still_needs_food() {
        let mut p = Pet::new("Mochi".into(), 1000);
        p.energy = 10;
        p.sleeping = true;
        p.advance(7000);
        assert_eq!(p.energy, 70);
        assert_eq!(p.fullness, 70);
        assert!(p.act(Action::Feed, 7000).contains("asleep"));
    }
    #[test]
    fn neglect_is_terminal_and_catchup_is_bounded() {
        let mut p = Pet::new("Mochi".into(), 1000);
        p.fullness = 20;
        assert_eq!(p.advance(1000 + 86400), 72);
        assert!(!p.alive());
        assert_eq!(p.last_tick, 87400);
        p.act(Action::Medicine, 87410);
        assert!(!p.alive());
        assert_eq!(p.advance(87410), 0);
        let death = p.died_at.unwrap();
        assert_eq!(p.age(200000), death - p.born);
    }
    #[test]
    fn care_has_limits_and_cooldown() {
        let mut p = Pet::new("Mochi".into(), 1000);
        p.act(Action::Feed, 1000);
        assert_eq!(p.fullness, 100);
        p.act(Action::Play, 1001);
        assert_eq!(p.energy, 80);
        p.act(Action::Play, 1005);
        assert_eq!(p.energy, 65);
        assert_eq!(p.happiness, 100);
    }
    #[test]
    fn a_rested_well_cared_for_buddy_can_sleep_through_the_night() {
        let mut p = Pet::new("Mochi".into(), 1000);
        p.sleeping = true;
        p.advance(1000 + 8 * 3600);
        assert!(p.alive());
        assert_eq!(p.health, 100);
        assert_eq!(p.energy, 100);
        assert_eq!(p.fullness, 32);
        assert_eq!(p.hygiene, 32);
    }

    #[test]
    fn only_helpful_care_earns_bond_points() {
        let mut p = Pet::new("Mochi".into(), 1000);
        p.act(Action::Feed, 1000);
        assert_eq!(p.care_points, 4);
        p.act(Action::Feed, 1005);
        p.act(Action::Medicine, 1010);
        p.act(Action::Sleep, 1015);
        p.act(Action::Sleep, 1016);
        assert_eq!(p.care_points, 4);
        p.care_points = 36;
        p.act(Action::Play, 1020);
        assert_eq!(p.bond_level(), 2);
        assert_eq!(p.progression(1020).bond_progress, 2);
        p.act(Action::Play, 1025);
        assert_eq!(p.care_points, 42, "Full happiness must not award points");
    }

    #[test]
    fn growth_progress_and_achievements_follow_age_and_bond() {
        let mut p = Pet::new("Mochi".into(), 1000);
        let baby = p.progression(2800);
        assert_eq!(baby.stage, "Baby");
        assert_eq!(baby.growth_percent, 50);
        assert_eq!(baby.seconds_until_growth, Some(1800));
        assert_eq!(p.progression(4600).stage, "Child");
        assert_eq!(p.progression(22600).stage, "Teen");
        p.care_points = 40;
        let adult = p.progression(87400);
        assert!(adult.next_stage.is_none());
        assert!(adult.achievements.iter().all(|a| a.unlocked));
        p.health = 0;
        p.died_at = Some(4600);
        let memorial = p.progression(200000);
        assert_eq!(memorial.stage, "Child");
        assert_eq!(memorial.seconds_until_growth, None);
        assert!(!memorial.achievements[2].unlocked);
    }
}

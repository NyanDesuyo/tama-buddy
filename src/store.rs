use crate::pet::{Action, Pet};
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::path::Path;

pub struct Store {
    connection: Connection,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).context("Could not create save directory")?;
        }
        let connection = Connection::open(path).context("Could not open SQLite save")?;
        Self::initialize(connection)
    }

    fn initialize(mut connection: Connection) -> Result<Self> {
        let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        anyhow::ensure!(
            version <= 2,
            "This save was created by a newer version of Tama Buddy."
        );
        connection.busy_timeout(std::time::Duration::from_secs(3))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS pet (
                id INTEGER PRIMARY KEY CHECK(id = 1), name TEXT NOT NULL,
                born INTEGER NOT NULL, last_tick INTEGER NOT NULL,
                fullness INTEGER NOT NULL, happiness INTEGER NOT NULL,
                energy INTEGER NOT NULL, hygiene INTEGER NOT NULL,
                health INTEGER NOT NULL, sleeping INTEGER NOT NULL,
                last_action INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY, at INTEGER NOT NULL, message TEXT NOT NULL);
",
        )?;
        let migration = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let columns: Vec<String> = {
            let mut statement = migration.prepare("PRAGMA table_info(pet)")?;
            statement
                .query_map([], |r| r.get(1))?
                .collect::<rusqlite::Result<_>>()?
        };
        if !columns.iter().any(|c| c == "care_points") {
            migration.execute_batch(
                "ALTER TABLE pet ADD COLUMN care_points INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        if !columns.iter().any(|c| c == "died_at") {
            migration.execute_batch("ALTER TABLE pet ADD COLUMN died_at INTEGER;")?;
        }
        migration.execute_batch("UPDATE pet SET died_at=last_tick WHERE health<=0 AND died_at IS NULL; PRAGMA user_version=2;")?;
        migration.commit()?;
        Ok(Self { connection })
    }
    fn load(connection: &Connection) -> Result<Option<Pet>> {
        Ok(connection.query_row(
            "SELECT name,born,last_tick,fullness,happiness,energy,hygiene,health,sleeping,last_action,care_points,died_at FROM pet WHERE id=1", [],
            |r| Ok(Pet { name:r.get(0)?, born:r.get(1)?, last_tick:r.get(2)?, fullness:r.get(3)?,
                happiness:r.get(4)?, energy:r.get(5)?, hygiene:r.get(6)?, health:r.get(7)?,
                sleeping:r.get(8)?, last_action:r.get(9)?, care_points:r.get(10)?, died_at:r.get(11)? })
        ).optional()?)
    }
    fn save(tx: &Connection, pet: &Pet, event: Option<&str>, now: i64) -> Result<()> {
        tx.execute(
            "INSERT INTO pet(id,name,born,last_tick,fullness,happiness,energy,hygiene,health,sleeping,last_action,care_points,died_at) VALUES(1,?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
            ON CONFLICT(id) DO UPDATE SET name=excluded.name,born=excluded.born,
            last_tick=excluded.last_tick,fullness=excluded.fullness,happiness=excluded.happiness,
            energy=excluded.energy,hygiene=excluded.hygiene,health=excluded.health,
            sleeping=excluded.sleeping,last_action=excluded.last_action,care_points=excluded.care_points,died_at=excluded.died_at",
            params![
                pet.name,
                pet.born,
                pet.last_tick,
                pet.fullness,
                pet.happiness,
                pet.energy,
                pet.hygiene,
                pet.health,
                pet.sleeping,
                pet.last_action,
                pet.care_points,
                pet.died_at
            ],
        )?;
        if let Some(message) = event {
            tx.execute(
                "INSERT INTO events(at,message) VALUES(?1,?2)",
                params![now, message],
            )?;
            tx.execute("DELETE FROM events WHERE id NOT IN (SELECT id FROM events ORDER BY id DESC LIMIT 50)", [])?;
        }
        Ok(())
    }

    // Lock BEFORE reading, so backend ticks and TUI actions always use the latest
    // committed state. Never write a cached UI snapshot back to the database.
    fn update(
        &mut self,
        now: i64,
        operation: impl FnOnce(&mut Option<Pet>) -> Option<String>,
    ) -> Result<(Option<Pet>, Option<String>)> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut pet = Self::load(&tx)?;
        let original = pet.clone();
        let mut message = None;
        if let Some(p) = &mut pet {
            let alive = p.alive();
            let previous_stage = p.stage(p.last_tick);
            p.advance(now);
            if p.alive() && p.stage(p.last_tick) != previous_stage {
                message = Some(format!("{} grew into a {}!", p.name, p.stage(now)));
                Self::save(&tx, p, message.as_deref(), now)?;
            }
            if alive && !p.alive() {
                message = Some("Your buddy is resting in the stars. A new egg is waiting.".into());
                Self::save(&tx, p, message.as_deref(), now)?;
            }
        }
        if let Some(action_message) = operation(&mut pet) {
            message = Some(action_message);
            if let Some(p) = &pet {
                Self::save(&tx, p, message.as_deref(), now)?;
            }
        } else if pet != original
            && let Some(p) = &pet
        {
            Self::save(&tx, p, None, now)?;
        }
        if let Some(p) = &pet
            && original
                .as_ref()
                .is_some_and(|old| old.born == p.born && old.bond_level() < p.bond_level())
        {
            let milestone = format!(
                "{} reached bond level {}. Growing closer, one little moment at a time!",
                p.name,
                p.bond_level()
            );
            Self::save(&tx, p, Some(&milestone), now)?;
            message = Some(
                format!("{} {milestone}", message.unwrap_or_default())
                    .trim()
                    .to_owned(),
            );
        }
        tx.commit()?;
        Ok((pet, message))
    }

    pub fn refresh(&mut self, now: i64) -> Result<(Option<Pet>, Option<String>)> {
        self.update(now, |_| None)
    }

    pub fn care(&mut self, action: Action, now: i64) -> Result<(Option<Pet>, Option<String>)> {
        self.update(now, |pet| {
            Some(match pet {
                Some(p) => p.act(action, now),
                None => "Hatch a buddy first.".into(),
            })
        })
    }

    pub fn hatch(&mut self, name: String, now: i64) -> Result<(Option<Pet>, Option<String>)> {
        self.update(now, |pet| {
            if pet.as_ref().is_some_and(Pet::alive) {
                return Some("A buddy is already living here. Welcome home!".into());
            }
            let message = format!("{name} hatched! A tiny friendship begins.");
            *pet = Some(Pet::new(name, now));
            Some(message)
        })
    }

    pub fn events(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .connection
            .prepare("SELECT message FROM events ORDER BY id DESC LIMIT 5")?;
        Ok(stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sqlite_roundtrip_replacement_and_event_retention() -> Result<()> {
        let store = Store::open(Path::new(":memory:"))?;
        assert!(Store::load(&store.connection)?.is_none());
        let mut p = Pet::new("もち ' Mochi".into(), 1000);
        p.sleeping = true;
        Store::save(&store.connection, &p, Some("Hatched!"), 1000)?;
        assert_eq!(Store::load(&store.connection)?.as_ref(), Some(&p));
        for i in 0..60 {
            p = Pet::new(format!("Buddy {i}"), 2000 + i);
            Store::save(&store.connection, &p, Some(&format!("Event {i}")), 2000 + i)?;
        }
        assert_eq!(Store::load(&store.connection)?.as_ref(), Some(&p));
        assert_eq!(store.events()?[0], "Event 59");
        let count: i64 = store
            .connection
            .query_row("SELECT count(*) FROM events", [], |r| r.get(0))?;
        assert_eq!(count, 50);
        Ok(())
    }
    #[test]
    fn existing_save_migrates_without_replacing_the_pet() -> Result<()> {
        let store = Store::open(Path::new(":memory:"))?;
        let pet = Pet::new("Old friend".into(), 1000);
        Store::save(&store.connection, &pet, Some("An old memory"), 1000)?;
        store.connection.execute_batch(
            "ALTER TABLE pet DROP COLUMN care_points;
            ALTER TABLE pet DROP COLUMN died_at; PRAGMA user_version=1;",
        )?;
        let migrated = Store::initialize(store.connection)?;
        assert_eq!(Store::load(&migrated.connection)?, Some(pet));
        assert_eq!(migrated.events()?[0], "An old memory");
        let version: i64 = migrated
            .connection
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        assert_eq!(version, 2);
        Ok(())
    }

    #[test]
    fn growth_and_bond_milestones_are_saved_once() -> Result<()> {
        let mut store = Store::open(Path::new(":memory:"))?;
        let mut pet = Pet::new("Mochi".into(), 1000);
        pet.care_points = 36;
        Store::save(&store.connection, &pet, None, 1000)?;
        let (_, message) = store.care(Action::Feed, 1000)?;
        assert!(message.unwrap().contains("bond level 2"));
        store.refresh(4600)?;
        store.refresh(4601)?;
        let messages = store.events()?;
        assert_eq!(
            messages
                .iter()
                .filter(|m| m.contains("grew into a Child"))
                .count(),
            1
        );
        assert_eq!(
            messages
                .iter()
                .filter(|m| m.contains("bond level 2"))
                .count(),
            1
        );
        Ok(())
    }
}

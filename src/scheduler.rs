//! Schedules: start, stop, restart and commands on a timer, Steam updates, and Rust's wipes.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::rust_wipe::Wipe;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    Interval {
        every_seconds: u64,
    },
    /// Five fields in local time; both day fields must match, so `0 19 1-7 * 4` is the first Thursday.
    Cron {
        expression: String,
    },
    After {
        schedule_id: String,
        delay_seconds: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Start,
    Stop {
        #[serde(default)]
        warn_seconds: u64,
    },
    Restart {
        #[serde(default)]
        warn_seconds: u64,
    },
    Command {
        command: String,
    },
    Wipe {
        kind: Wipe,
        #[serde(default)]
        new_seed: bool,
        #[serde(default)]
        warn_seconds: u64,
    },
    Update {
        #[serde(default)]
        warn_seconds: u64,
        /// Only when Steam has a build the server does not, which suits a frequent schedule.
        #[serde(default)]
        only_if_newer: bool,
    },
}

const SHORTEST_INTERVAL: u64 = 60;
const LONGEST_WARNING: u64 = 30 * 60;

fn cron(expression: &str) -> Result<croner::Cron> {
    croner::parser::CronParser::builder()
        .dom_and_dow(true)
        .build()
        .parse(expression)
        .with_context(|| format!("'{expression}' is not a cron line"))
}

/// When a schedule next runs after `after`. A chained one waits for the one before it.
pub fn next_run(trigger: &Trigger, after: DateTime<Utc>) -> Result<Option<DateTime<Utc>>> {
    Ok(match trigger {
        Trigger::Interval { every_seconds } => {
            Some(after + chrono::Duration::seconds(*every_seconds as i64))
        }
        Trigger::Cron { expression } => {
            let next = cron(expression)?
                .find_next_occurrence(&after.with_timezone(&Local), false)
                .with_context(|| format!("'{expression}' never comes round"))?;
            Some(next.with_timezone(&Utc))
        }
        Trigger::After { .. } => None,
    })
}

/// Whether a schedule makes sense for this kind of server, as a message for the person.
pub fn check(trigger: &Trigger, action: &Action, kind: &str) -> std::result::Result<(), String> {
    match trigger {
        Trigger::Interval { every_seconds } if *every_seconds < SHORTEST_INTERVAL => {
            return Err("Run it a minute apart at the least.".into());
        }
        Trigger::Cron { expression } => {
            cron(expression).map_err(|error| format!("{error:#}"))?;
        }
        _ => {}
    }
    let warning = match action {
        Action::Stop { warn_seconds }
        | Action::Restart { warn_seconds }
        | Action::Wipe { warn_seconds, .. }
        | Action::Update { warn_seconds, .. } => *warn_seconds,
        Action::Start | Action::Command { .. } => 0,
    };
    if warning > LONGEST_WARNING {
        return Err("Give players half an hour's warning at the most.".into());
    }
    match action {
        Action::Command { .. } if !crate::steam_games::takes_commands(kind) => {
            Err("This game takes no commands.".into())
        }
        Action::Command { command } if command.trim().is_empty() => {
            Err("Say which command to run.".into())
        }
        Action::Command { command } if command.chars().any(char::is_control) => {
            Err("A command is one line.".into())
        }
        Action::Wipe { .. } if kind != crate::rust_game::KIND => {
            Err("Wipes are for Rust servers.".into())
        }
        Action::Update { .. } if !crate::steam_games::is_steam(kind) => {
            Err("Steam updates are for games installed through Steam.".into())
        }
        _ => Ok(()),
    }
}

/// Checks for due schedules every few seconds and runs them.
pub async fn run_due(state: AppState) {
    loop {
        tokio::time::sleep(Duration::from_secs(15)).await;
        let now = Utc::now();
        let due: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM schedules WHERE enabled = 1 AND next_run_at IS NOT NULL AND next_run_at <= ?",
        )
        .bind(now.to_rfc3339())
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        for (id,) in due {
            let state = state.clone();
            tokio::spawn(async move {
                if let Err(error) = run(&state, &id).await {
                    tracing::warn!(%error, schedule = %id, "a schedule could not run");
                }
            });
        }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    server_id: String,
    trigger: String,
    action: String,
    one_time: bool,
}

/// Runs one schedule now, then sets when it runs next and wakes anything chained to it.
pub async fn run(state: &AppState, schedule_id: &str) -> Result<()> {
    let row: Row =
        sqlx::query_as("SELECT server_id, trigger, action, one_time FROM schedules WHERE id = ?")
            .bind(schedule_id)
            .fetch_one(&state.db)
            .await
            .context("loading the schedule")?;
    let trigger: Trigger = serde_json::from_str(&row.trigger)?;
    let action: Action = serde_json::from_str(&row.action)?;
    let server = Uuid::parse_str(&row.server_id)?;

    // Moved on before the work starts, so a slow restart is not picked up twice.
    let started = Utc::now();
    let next = match row.one_time {
        true => None,
        false => next_run(&trigger, started)?,
    };
    sqlx::query(
        "UPDATE schedules SET next_run_at = CASE WHEN enabled = 1 THEN ? END,
             enabled = CASE WHEN one_time = 1 THEN 0 ELSE enabled END,
             last_run_at = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(next.map(|at| at.to_rfc3339()))
    .bind(started.to_rfc3339())
    .bind(started.to_rfc3339())
    .bind(schedule_id)
    .execute(&state.db)
    .await?;

    let outcome = perform(state, server, &action).await;
    let result = match &outcome {
        Ok(said) => said.clone(),
        Err(error) => format!("Failed: {error:#}"),
    };
    sqlx::query("UPDATE schedules SET last_result = ? WHERE id = ?")
        .bind(&result)
        .bind(schedule_id)
        .execute(&state.db)
        .await?;

    // What waits on this one starts its own clock now.
    let chained: Vec<(String, String)> =
        sqlx::query_as("SELECT id, trigger FROM schedules WHERE server_id = ? AND enabled = 1")
            .bind(&row.server_id)
            .fetch_all(&state.db)
            .await?;
    for (id, trigger) in chained {
        if let Ok(Trigger::After {
            schedule_id: parent,
            delay_seconds,
        }) = serde_json::from_str(&trigger)
            && parent == schedule_id
        {
            let at = Utc::now() + chrono::Duration::seconds(delay_seconds as i64);
            sqlx::query("UPDATE schedules SET next_run_at = ? WHERE id = ?")
                .bind(at.to_rfc3339())
                .bind(&id)
                .execute(&state.db)
                .await?;
        }
    }
    outcome.map(|_| ())
}

async fn perform(state: &AppState, id: Uuid, action: &Action) -> Result<String> {
    let live = state.supervisor.state(id).await.is_live();
    match action {
        Action::Start if live => Ok("Already running.".into()),
        Action::Start => {
            state.supervisor.start_by_id(id).await?;
            Ok("Started.".into())
        }
        Action::Stop { .. } if !live => Ok("Already stopped.".into()),
        Action::Stop { warn_seconds } => {
            countdown(state, id, *warn_seconds, "stops").await;
            state.supervisor.stop(id).await?;
            Ok("Stopped.".into())
        }
        Action::Restart { warn_seconds } => {
            if live {
                countdown(state, id, *warn_seconds, "restarts").await;
            }
            state.supervisor.restart(id).await?;
            Ok("Restarted.".into())
        }
        Action::Command { .. } if !live => Ok("Skipped: the server was not running.".into()),
        Action::Command { command } => {
            let outcome = state.supervisor.run_command(id, command).await?;
            Ok(outcome
                .output
                .filter(|said| !said.is_empty())
                .unwrap_or_else(|| "Sent.".into()))
        }
        Action::Wipe {
            kind,
            new_seed,
            warn_seconds,
        } => {
            if live {
                countdown(state, id, *warn_seconds, "wipes").await;
                state.supervisor.stop(id).await?;
            }
            let (deleted, seed) =
                crate::rust_wipe::wipe_server(state, id, *kind, *new_seed).await?;
            if live {
                state.supervisor.start_by_id(id).await?;
            }
            Ok(match seed {
                Some(seed) => format!("Deleted {} files; the new seed is {seed}.", deleted.len()),
                None => format!("Deleted {} files.", deleted.len()),
            })
        }
        Action::Update {
            warn_seconds,
            only_if_newer,
        } => {
            let (kind, directory, branch): (String, String, Option<String>) = sqlx::query_as(
                "SELECT kind, directory, provider_version FROM servers WHERE id = ?",
            )
            .bind(id.to_string())
            .fetch_one(&state.db)
            .await?;
            let branch = branch.unwrap_or_default();
            let directory = std::path::PathBuf::from(directory);
            if *only_if_newer {
                let build = crate::steam_updates::check(state, &kind, &directory, &branch).await?;
                if !build.update_available {
                    return Ok("Already on the newest build.".into());
                }
            }
            let runtime = state.supervisor.runtime(id).await;
            if runtime.installing || runtime.updating {
                bail!("the server is already being installed or updated");
            }
            crate::steam_updates::spawn_update(
                state.clone(),
                id,
                kind,
                directory,
                branch,
                *warn_seconds,
            );
            Ok("Updating from Steam.".into())
        }
    }
}

/// Counts down in chat, as in "The server restarts in a minute", then at one minute and ten seconds.
pub async fn countdown(state: &AppState, id: Uuid, seconds: u64, what: &str) {
    let kind: String = sqlx::query_scalar("SELECT kind FROM servers WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(&state.db)
        .await
        .unwrap_or_default();
    // A game that takes no commands cannot be told, so waiting would only delay it.
    if !crate::steam_games::takes_commands(&kind) {
        return;
    }
    let mut marks = vec![seconds, 60, 10];
    marks.retain(|mark| *mark > 0 && *mark <= seconds);
    marks.dedup();
    let mut left = seconds;
    for mark in marks {
        tokio::time::sleep(Duration::from_secs(left - mark)).await;
        left = mark;
        let when = match mark.div_ceil(60) {
            _ if mark < 60 => format!("in {mark} seconds"),
            1 => "in a minute".to_string(),
            minutes => format!("in {minutes} minutes"),
        };
        let message = match what.split_once(' ') {
            Some((verb, reason)) => format!("say The server {verb} {when} {reason}."),
            None => format!("say The server {what} {when}."),
        };
        state.supervisor.run_command(id, &message).await.ok();
    }
    tokio::time::sleep(Duration::from_secs(left)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone, Timelike};

    #[test]
    fn an_interval_runs_that_long_after() {
        let at = Utc.with_ymd_and_hms(2026, 10, 5, 12, 0, 0).unwrap();
        let next = next_run(
            &Trigger::Interval {
                every_seconds: 3600,
            },
            at,
        )
        .unwrap();
        assert_eq!(next, Some(at + chrono::Duration::hours(1)));
    }

    #[test]
    fn both_day_fields_have_to_match() {
        let at = Local.with_ymd_and_hms(2026, 10, 5, 12, 0, 0).unwrap();
        let trigger = Trigger::Cron {
            expression: "0 19 1-7 * 4".into(),
        };
        let next = next_run(&trigger, at.with_timezone(&Utc))
            .unwrap()
            .unwrap()
            .with_timezone(&Local);
        // The first Thursday after the 5th of October 2026 is the 5th of November.
        assert_eq!((next.month(), next.day(), next.hour()), (11, 5, 19));
    }

    #[test]
    fn a_chained_schedule_waits() {
        let trigger = Trigger::After {
            schedule_id: "x".into(),
            delay_seconds: 60,
        };
        assert_eq!(next_run(&trigger, Utc::now()).unwrap(), None);
    }

    #[test]
    fn refuses_what_does_not_fit() {
        let every = |seconds| Trigger::Interval {
            every_seconds: seconds,
        };
        assert!(check(&every(30), &Action::Start, "rust").is_err());
        let bad_cron = Trigger::Cron {
            expression: "every day".into(),
        };
        assert!(check(&bad_cron, &Action::Start, "rust").is_err());
        let wipe = Action::Wipe {
            kind: Wipe::Map,
            new_seed: true,
            warn_seconds: 300,
        };
        assert!(check(&every(3600), &wipe, "minecraft_java").is_err());
        assert!(check(&every(3600), &wipe, "rust").is_ok());
        let command = Action::Command {
            command: "say a\nstop".into(),
        };
        assert!(check(&every(3600), &command, "minecraft_java").is_err());
    }
}

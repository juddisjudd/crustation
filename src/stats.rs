use std::collections::HashMap;

use chrono::Utc;
use serde_json::json;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use uuid::Uuid;

use crate::state::AppState;

/// Samples running servers on a timer, publishes the numbers and keeps history
/// for the charts.
pub async fn collect(state: AppState) {
    let interval = std::time::Duration::from_secs(state.config.panel.stats_interval_seconds.max(2));
    let mut ticker = tokio::time::interval(interval);
    let mut system = System::new();
    let cores = std::thread::available_parallelism()
        .map(|value| value.get() as f32)
        .unwrap_or(1.0);
    let mut since_prune = 0u32;

    loop {
        ticker.tick().await;

        // Every server is asked, not only the ones the panel started, so a server
        // somebody else launched on that port still shows up.
        let players = ping_all(&state).await;

        let running = state.supervisor.running_ids().await;
        if running.is_empty() {
            publish_host(&state, &mut system);
            continue;
        }

        let mut pids = Vec::new();
        for id in &running {
            if let Some(pid) = state.supervisor.pid(*id).await {
                pids.push((*id, Pid::from_u32(pid)));
            }
        }

        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&pids.iter().map(|(_, pid)| *pid).collect::<Vec<_>>()),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        system.refresh_memory();
        let total_memory = system.total_memory().max(1);

        for (id, pid) in pids {
            let Some(process) = system.process(pid) else {
                continue;
            };
            // sysinfo reports CPU per core; normalise so 100% means the whole machine.
            let cpu = (process.cpu_usage() / cores) as f64;
            let memory = process.memory();
            let memory_percent = memory as f64 / total_memory as f64 * 100.0;
            let at = Utc::now();

            let seen = players.get(&id);
            let online = seen.map(|status| status.players_online);
            let max = seen.map(|status| status.players_max);

            let result = sqlx::query(
                "INSERT OR REPLACE INTO server_stats
                 (server_id, at, cpu_percent, memory_bytes, memory_percent, players_online, players_max)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id.to_string())
            .bind(at.to_rfc3339())
            .bind(cpu)
            .bind(memory as i64)
            .bind(memory_percent)
            .bind(online)
            .bind(max)
            .execute(&state.db)
            .await;

            if let Err(error) = result {
                tracing::debug!(%error, "could not store stats");
            }

            state.events.server_stats(
                id,
                json!({
                    "server_id": id,
                    "at": at,
                    "cpu_percent": cpu,
                    "memory_bytes": memory,
                    "memory_percent": memory_percent,
                    "players_online": online,
                    "players_max": max,
                    "version": seen.map(|status| status.version.clone()),
                    "motd": seen.map(|status| status.motd.clone()),
                    "latency_ms": seen.map(|status| status.latency_ms),
                }),
            );
        }

        publish_host(&state, &mut system);

        since_prune += 1;
        if since_prune >= 360 {
            since_prune = 0;
            prune(&state).await;
        }
    }
}

/// Asks every configured server what it is, all at once so one slow reply does
/// not hold up the tick.
async fn ping_all(state: &AppState) -> HashMap<Uuid, crate::ping::Status> {
    let rows: Vec<(String, String, String, i64)> =
        match sqlx::query_as("SELECT id, kind, host, port FROM servers")
            .fetch_all(&state.db)
            .await
        {
            Ok(rows) => rows,
            Err(error) => {
                tracing::debug!(%error, "could not list servers to ping");
                return HashMap::new();
            }
        };

    let answers = futures::future::join_all(rows.into_iter().map(|(id, kind, host, port)| {
        let state = state.clone();
        async move {
            let id = Uuid::parse_str(&id).ok()?;
            let port = u16::try_from(port).ok()?;
            let status = crate::ping::query(&kind, &host, port).await.ok();
            state.statuses.record(id, status.clone()).await;
            Some((id, status?))
        }
    }))
    .await;

    let seen: HashMap<Uuid, crate::ping::Status> = answers.into_iter().flatten().collect();
    for (id, status) in &seen {
        remember_players(state, *id, status).await;
    }
    seen
}

/// Keeps a roll of who has been on. Java hands back a short sample rather than
/// the whole list, and none at all when `hide-online-players` is set, so this is
/// what the server was willing to say and not a register.
async fn remember_players(state: &AppState, id: Uuid, status: &crate::ping::Status) {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query("UPDATE server_players SET online = 0 WHERE server_id = ?")
        .bind(id.to_string())
        .execute(&state.db)
        .await;
    if let Err(error) = result {
        tracing::debug!(%error, "could not clear the online marks");
        return;
    }

    for player in &status.sample {
        let stored = sqlx::query(
            "INSERT INTO server_players (server_id, name, uuid, first_seen, last_seen, online)
             VALUES (?, ?, ?, ?, ?, 1)
             ON CONFLICT(server_id, name) DO UPDATE SET
                 last_seen = excluded.last_seen,
                 online = 1,
                 uuid = COALESCE(excluded.uuid, server_players.uuid)",
        )
        .bind(id.to_string())
        .bind(&player.name)
        .bind(&player.uuid)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await;

        if let Err(error) = stored {
            tracing::debug!(%error, "could not remember a player");
        }
    }
}

fn publish_host(state: &AppState, system: &mut System) {
    system.refresh_memory();
    system.refresh_cpu_usage();
    let total = system.total_memory().max(1);
    let used = total.saturating_sub(system.available_memory());
    state.events.publish(
        crate::events::Topic::Panel,
        "host_stats",
        json!({
            "cpu_percent": system.global_cpu_usage(),
            "memory_total_bytes": total,
            "memory_used_bytes": used,
            "memory_used_percent": used as f64 / total as f64 * 100.0,
        }),
    );
}

async fn prune(state: &AppState) {
    let cutoff = Utc::now() - chrono::Duration::days(state.config.panel.stats_retention_days);
    if let Err(error) = sqlx::query("DELETE FROM server_stats WHERE at < ?")
        .bind(cutoff.to_rfc3339())
        .execute(&state.db)
        .await
    {
        tracing::warn!(%error, "could not prune old stats");
    }
}

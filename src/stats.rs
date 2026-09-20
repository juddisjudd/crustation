use chrono::Utc;
use serde_json::json;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

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

            let result = sqlx::query(
                "INSERT OR REPLACE INTO server_stats
                 (server_id, at, cpu_percent, memory_bytes, memory_percent, players_online, players_max)
                 VALUES (?, ?, ?, ?, ?, NULL, NULL)",
            )
            .bind(id.to_string())
            .bind(at.to_rfc3339())
            .bind(cpu)
            .bind(memory as i64)
            .bind(memory_percent)
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

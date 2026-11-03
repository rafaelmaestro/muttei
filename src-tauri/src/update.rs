use crate::Core;
use std::sync::Arc;
use std::time::Duration;
use tauri_plugin_updater::UpdaterExt;

const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

pub fn watch(core: Arc<Core>) {
    tauri::async_runtime::spawn(async move {
        // let the network and the tray settle after login
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            check(&core, false).await;
            tokio::time::sleep(EVERY).await;
        }
    });
}

pub async fn check(core: &Arc<Core>, manual: bool) {
    if manual {
        core.set_upd_stage("checking");
    }
    let found = match core.app.updater() {
        Ok(u) => u.check().await.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    match found {
        Ok(Some(u)) => {
            let v = u.version.clone();
            let known = core.update.lock().unwrap().replace(u).is_some_and(|old| old.version == v);
            core.set_upd_stage("idle");
            if !known {
                core.pill_icon("on", &format!("Versão {v} disponível"), "Ajustes > Geral", "up");
                core.tray_update(&v);
            }
        }
        Ok(None) => {
            core.set_upd_stage("idle");
            if manual {
                core.pill("on", "O Muttei está em dia", &core.app.package_info().version.to_string());
            }
        }
        Err(e) => {
            eprintln!("muttei: update check failed: {e}");
            core.set_upd_stage(if manual { "error" } else { "idle" });
        }
    }
}

pub async fn install(core: Arc<Core>) {
    let Some(u) = core.update.lock().unwrap().clone() else { return };
    let (mut got, mut shown) = (0u64, u64::MAX);
    let res = u
        .download_and_install(
            |chunk, total| {
                got += chunk as u64;
                let pct = total.filter(|&t| t > 0).map(|t| got * 100 / t).unwrap_or(0);
                if pct != shown {
                    shown = pct;
                    core.set_upd_stage(&format!("down:{pct}"));
                }
            },
            || core.set_upd_stage("installing"),
        )
        .await;
    match res {
        Ok(()) => core.app.restart(),
        Err(e) => {
            eprintln!("muttei: update install failed: {e}");
            core.set_upd_stage("error");
            core.pill("warn", "A atualização falhou", "tente de novo em Ajustes > Geral");
        }
    }
}

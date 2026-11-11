use authority::{AuthorityProxy, Subject};
use dbus::AuthenticationAgent;
use eyre::{Context, Result, ensure};
use futures_lite::StreamExt;
use gtk4::glib::{clone, spawn_future_local};
use relm4::RelmApp;
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::mpsc::channel;
use tracing::level_filters::LevelFilter;
use zbus::conn;
use zbus::zvariant::Value;

use crate::config::SystemConfig;
use crate::events::{AuthenticationAgentEvent, AuthenticationUserEvent};
use crate::ui::{App, KANAGAWA_CSS};

mod authority;
mod config;
mod constants;
mod dbus;
mod events;
mod ui;

fn setup_tracing() -> Result<()> {
    let subscriber = tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    Ok(())
}

fn resolve_session_id() -> String {
    // 1. Проверяем переменную окружения XDG_SESSION_ID
    if let Ok(id) = std::env::var("XDG_SESSION_ID") {
        if !id.trim().is_empty() {
            return id.trim().to_string();
        }
    }

    // 2. Опрашиваем loginctl для поиска графической сессии пользователя seat0
    if let Ok(output) = std::process::Command::new("loginctl")
        .args(["list-sessions", "--no-legend"])
        .output()
    {
        if let Ok(text) = String::from_utf8(output.stdout) {
            for line in text.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 && parts[3] == "seat0" {
                    return parts[0].to_string();
                }
            }
        }
    }

    // 3. Fallback на ID текущей сессии
    if let Ok(id) = std::fs::read_to_string("/proc/self/sessionid") {
        let trimmed = id.trim();
        if !trimmed.is_empty() && trimmed != "4294967295" {
            return trimmed.to_string();
        }
    }

    "3".to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_tracing()?;

    let config: SystemConfig = SystemConfig::from_file()?;

    let agent_helper = Path::new(config.get_helper_path());
    let agent_socket = Path::new(config.get_socket_path());

    ensure!(
        agent_helper.exists() || agent_socket.exists(),
        "Helper located at {} and socket located at {} do not exist.",
        &agent_helper.display(),
        &agent_socket.display()
    );

    if agent_socket.exists() {
        tracing::info!("using agent socket at {}", &agent_socket.display());
    } else {
        tracing::info!("using agent helper at {}", &agent_helper.display());
    }

    let (agent_sender, agent_receiver) = channel::<AuthenticationAgentEvent>(32);
    let (user_sender, user_receiver) = channel::<AuthenticationUserEvent>(32);

    let locale = gtk4::glib::language_names()[0].as_str().to_string();
    let session_id = resolve_session_id();
    tracing::info!("Registering authentication agent for session {} with locale: {}", session_id, locale);

    let subject_kind = "unix-session".to_string();
    let subject_details = HashMap::from([("session-id".to_string(), Value::new(session_id))]);
    let subject = Subject::new(subject_kind, subject_details);

    let agent = AuthenticationAgent::new(agent_sender, user_receiver, config.clone());
    let connection = conn::Builder::system()?
        .serve_at(constants::SELF_OBJECT_PATH, agent)?
        .build()
        .await?;

    let proxy = AuthorityProxy::new(&connection).await?;
    proxy
        .register_authentication_agent(&subject, &locale, constants::SELF_OBJECT_PATH)
        .await?;

    tracing::info!("Registered as authentication agent.");

    let mut owner_change_signal = proxy
        .inner()
        .receive_owner_changed()
        .await
        .context("Could not get the signal for owner change?")?;

    spawn_future_local(clone!(async move {
        while let Some(result) = owner_change_signal.next().await {
            if result.is_some() {
                tracing::info!("Polkit owner changed, reregistering.");
                let _ = proxy
                    .register_authentication_agent(&subject, &locale, constants::SELF_OBJECT_PATH)
                    .await;
            }
        }
    }));

    let app = RelmApp::new("org.kanagawa.polkit");

    // Внедряем встроенный стиль Kanagawa Dragon
    relm4::set_global_css(KANAGAWA_CSS);

    // Дополнительный пользовательский стиль (если задан)
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .or_else(|_| std::env::var("HOME").map(|h| format!("{}/.config", h)))
        .unwrap_or_default();
    let custom_css = format!("{}/kanagawa-polkit/style.css", config_dir);
    if Path::new(&custom_css).is_file()
        && relm4::set_global_css_from_file(Path::new(&custom_css)).is_ok()
    {
        tracing::info!("Loaded custom CSS from {}", custom_css);
    }

    app.run_async::<App>((user_sender, agent_receiver));

    Ok(())
}

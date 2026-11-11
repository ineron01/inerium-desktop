mod model;
mod system;
mod theme;
mod ui;

use gtk4::prelude::*;
use gtk4::{Application, CssProvider, Settings, glib};

use crate::model::{SetupPaths, reset_all_markers};
use crate::system::should_open_autostart;

fn print_usage() {
    println!(
        "Usage: kanagawa-setup [OPTIONS]\n\
         \n\
         Options:\n\
           --autostart            Open setup only if wizard or first-login.d steps are pending\n\
           --reset                Reset first-login and wizard completion markers\n\
           --tab <name>           Open specific tab (displays | interface | integrations)\n\
           -h, --help             Show this help"
    );
}

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut autostart = false;
    let mut reset = false;
    let mut initial_tab: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--autostart" => autostart = true,
            "--reset" => reset = true,
            "--tab" => {
                i += 1;
                if let Some(val) = args.get(i) {
                    initial_tab = Some(val.clone());
                } else {
                    eprintln!("--tab requires an argument");
                    return glib::ExitCode::FAILURE;
                }
            }
            "-h" | "--help" => {
                print_usage();
                return glib::ExitCode::SUCCESS;
            }
            other => {
                eprintln!("Unknown option: {other}");
                print_usage();
                return glib::ExitCode::FAILURE;
            }
        }
        i += 1;
    }

    let paths = SetupPaths::from_env();

    if reset {
        if let Err(e) = reset_all_markers(&paths) {
            eprintln!("Ошибка сброса маркеров: {e}");
            return glib::ExitCode::FAILURE;
        }
        println!("Маркеры первичной настройки сброшены.");
        if !autostart {
            return glib::ExitCode::SUCCESS;
        }
    }

    let mut start_on_integrations = initial_tab.as_deref() == Some("integrations");

    if autostart {
        let (should_open, only_integrations) = should_open_autostart(&paths);
        if !should_open {
            return glib::ExitCode::SUCCESS;
        }
        if only_integrations && initial_tab.is_none() {
            start_on_integrations = true;
        }
    }

    let app = Application::builder()
        .application_id("org.kanagawa.setup")
        .build();

    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }

        if let Some(settings) = Settings::default() {
            settings.set_gtk_application_prefer_dark_theme(true);
        }

        if let Some(display) = gdk4::Display::default() {
            let provider = CssProvider::new();
            provider.load_from_data(theme::SETUP_CSS);
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let win = ui::build_main_window(app, paths.clone(), autostart, start_on_integrations);
        win.present();
    });

    app.run_with_args(&Vec::<String>::new())
}

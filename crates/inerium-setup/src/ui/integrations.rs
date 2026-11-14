use std::cell::RefCell;
use std::collections::HashSet;
use std::process::Command;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, Box, Button, Entry, Label, Orientation, PolicyType, ScrolledWindow, glib, pango,
};
use vte4::prelude::*;
use vte4::{PtyFlags, Terminal};

use crate::i18n::{Lang, tr};
use crate::model::{
    FirstLoginStep, SetupPaths, StepMode, StepStatus, mark_step_done,
};
use crate::system::{evaluate_steps, run_step_check};
use crate::theme::vte_palette;
use crate::ui::StatusCallback;

type RenderStepsCb = Rc<RefCell<Option<Rc<dyn Fn(bool)>>>>;

pub fn build_integrations_tab<FPending>(
    paths: SetupPaths,
    initial_steps: Vec<FirstLoginStep>,
    status_cb: StatusCallback,
    lang: Lang,
    on_pending_changed: FPending,
) -> Box
where
    FPending: Fn(usize) + 'static + Clone,
{
    let root = Box::new(Orientation::Vertical, 10);

    // Header card
    let head_card = Box::new(Orientation::Horizontal, 10);
    head_card.add_css_class("setup-card");

    let head_info = Box::new(Orientation::Vertical, 2);
    head_info.set_hexpand(true);
    let head_title = Label::new(Some(tr(
        lang,
        "Accounts & external integrations",
        "Аккаунты и внешние интеграции",
    )));
    head_title.add_css_class("card-title");
    head_title.set_xalign(0.0);
    head_info.append(&head_title);

    let head_sub = Label::new(Some(tr(
        lang,
        "Executable steps from ~/.config/inerium/first-login.d/ (checked via --check)",
        "Исполняемые шаги из ~/.config/inerium/first-login.d/ (проверка через --check)",
    )));
    head_sub.add_css_class("card-muted");
    head_sub.set_xalign(0.0);
    head_info.append(&head_sub);
    head_card.append(&head_info);

    let recheck_btn = Button::with_label(tr(lang, "󰑐 Recheck", "󰑐 Проверить снова"));
    recheck_btn.add_css_class("btn-secondary");
    head_card.append(&recheck_btn);
    root.append(&head_card);

    // Steps list scroll
    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .vexpand(true)
        .build();
    let list_box = Box::new(Orientation::Vertical, 8);
    scroll.set_child(Some(&list_box));
    root.append(&scroll);

    // Collapsible Embedded VTE-4 Terminal Drawer
    let vte_box = Box::new(Orientation::Vertical, 6);
    vte_box.add_css_class("vte-frame");
    vte_box.set_visible(false);

    let vte_head = Box::new(Orientation::Horizontal, 8);
    let vte_title = Label::new(Some(tr(lang, "Embedded terminal", "Встроенный терминал")));
    vte_title.add_css_class("card-meta");
    vte_title.set_hexpand(true);
    vte_title.set_xalign(0.0);
    vte_head.append(&vte_title);

    let vte_close_btn = Button::with_label(tr(lang, "Hide", "Скрыть"));
    vte_close_btn.add_css_class("btn-secondary");
    vte_head.append(&vte_close_btn);
    vte_box.append(&vte_head);

    let terminal = Terminal::new();
    terminal.set_hexpand(true);
    terminal.set_vexpand(false);
    terminal.set_size_request(-1, 220);
    terminal.set_scrollback_lines(2000);
    let font_desc = pango::FontDescription::from_string("JetBrainsMono Nerd Font 11");
    terminal.set_font(Some(&font_desc));
    let (fg, bg, pal) = vte_palette();
    let pal_refs: Vec<&gdk4::RGBA> = pal.iter().collect();
    terminal.set_colors(Some(&fg), Some(&bg), &pal_refs);

    let vte_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .build();
    vte_scroll.set_size_request(-1, 220);
    vte_scroll.set_child(Some(&terminal));
    vte_box.append(&vte_scroll);
    root.append(&vte_box);

    {
        let vte_box_c = vte_box.clone();
        vte_close_btn.connect_clicked(move |_| vte_box_c.set_visible(false));
    }

    let deferred_ids: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));
    let active_step: Rc<RefCell<Option<FirstLoginStep>>> = Rc::new(RefCell::new(None));
    let cached_steps: Rc<RefCell<Vec<FirstLoginStep>>> = Rc::new(RefCell::new(initial_steps));

    let render_steps: RenderStepsCb = Rc::new(RefCell::new(None));

    // Handle VTE child exit once
    {
        let active_step = active_step.clone();
        let paths = paths.clone();
        let status_cb = status_cb.clone();
        let render_steps = render_steps.clone();
        let vte_title = vte_title.clone();

        terminal.connect_child_exited(move |_, wait_status| {
            let Some(step) = active_step.borrow_mut().take() else {
                return;
            };
            let exited_zero = wait_status == 0;
            let exit_code = if wait_status >= 0 && (wait_status & 0x7f) == 0 {
                (wait_status >> 8) & 0xff
            } else {
                wait_status
            };
            let checked_ok = run_step_check(&step.path);
            if exited_zero || checked_ok {
                let _ = mark_step_done(&paths.first_login_state_dir(), &step.id);
                vte_title.set_text(&match lang {
                    Lang::En => format!("Completed: {}", step.title),
                    Lang::Ru => format!("Завершено: {}", step.title),
                });
                status_cb.ok(&match lang {
                    Lang::En => format!("Step '{}' completed", step.title),
                    Lang::Ru => format!("Шаг «{}» завершён", step.title),
                });
            } else {
                vte_title.set_text(&match lang {
                    Lang::En => format!("Incomplete: {}", step.title),
                    Lang::Ru => format!("Не завершено: {}", step.title),
                });
                status_cb.warn(&match lang {
                    Lang::En => format!(
                        "Step '{}' did not finish (code {}) — you can retry",
                        step.title, exit_code
                    ),
                    Lang::Ru => format!(
                        "Шаг «{}» не завершён (код {}) — можно повторить",
                        step.title, exit_code
                    ),
                });
            }
            if let Some(ref f) = *render_steps.borrow() {
                f(true);
            }
        });
    }

    let render_fn: Rc<dyn Fn(bool)> = {
        let list_box = list_box.clone();
        let cached_steps = cached_steps.clone();
        let deferred_ids = deferred_ids.clone();
        let active_step = active_step.clone();
        let paths = paths.clone();
        let status_cb = status_cb.clone();
        let on_pending_changed = on_pending_changed.clone();
        let terminal = terminal.clone();
        let vte_box = vte_box.clone();
        let vte_title = vte_title.clone();
        let render_steps_ref = render_steps.clone();

        Rc::new(move |rescan: bool| {
            if rescan {
                *cached_steps.borrow_mut() = evaluate_steps(&paths);
            }
            while let Some(child) = list_box.first_child() {
                list_box.remove(&child);
            }

            let mut steps = cached_steps.borrow().clone();
            let deferred = deferred_ids.borrow();
            let mut pending_count = 0usize;

            for step in &mut steps {
                if step.status == StepStatus::Pending && deferred.contains(&step.id) {
                    step.status = StepStatus::Deferred;
                }
                let is_gdrive = step.id.contains("gdrive") || step.id.contains("rclone");
                // Cloud storage is optional by default; don't count towards blocking pending
                if step.status == StepStatus::Pending && !is_gdrive {
                    pending_count += 1;
                }
            }
            on_pending_changed(pending_count);

            if steps.is_empty() {
                let empty_card = Box::new(Orientation::Vertical, 6);
                empty_card.add_css_class("setup-subcard");
                let lbl = Label::new(Some(tr(
                    lang,
                    "No extra integration steps",
                    "Дополнительных шагов интеграции нет",
                )));
                lbl.add_css_class("card-title");
                lbl.set_xalign(0.0);
                empty_card.append(&lbl);

                let sub = Label::new(Some(tr(
                    lang,
                    "No executable scripts found in ~/.config/inerium/first-login.d/.",
                    "В ~/.config/inerium/first-login.d/ не найдено исполняемых скриптов.",
                )));
                sub.add_css_class("card-muted");
                sub.set_xalign(0.0);
                empty_card.append(&sub);
                list_box.append(&empty_card);
                return;
            }

            for step in steps {
                let is_gdrive = step.id.contains("gdrive") || step.id.contains("rclone");

                if is_gdrive && step.status != StepStatus::Configured {
                    // Expandable interactive wizard card for Google Drive / Cloud storage
                    let card = Box::new(Orientation::Vertical, 8);
                    card.add_css_class("setup-subcard");

                    let top_row = Box::new(Orientation::Horizontal, 10);
                    let info = Box::new(Orientation::Vertical, 2);
                    info.set_hexpand(true);
                    let title = Label::new(Some(&step.title));
                    title.add_css_class("card-title");
                    title.set_xalign(0.0);
                    title.set_wrap(true);
                    info.append(&title);

                    let meta = Label::new(Some(tr(
                        lang,
                        "Cloud storage · Optional (setup later by default)",
                        "Облачное хранилище · Опционально (настроить позже по умолчанию)",
                    )));
                    meta.add_css_class("card-muted");
                    meta.set_xalign(0.0);
                    info.append(&meta);
                    top_row.append(&info);

                    let pill = Label::new(Some(tr(lang, "Optional", "Опционально")));
                    pill.add_css_class("pill");
                    pill.add_css_class("pill-deferred");
                    pill.set_valign(Align::Center);
                    top_row.append(&pill);

                    let toggle_btn = Button::with_label(tr(
                        lang,
                        "Connect Cloud (Advanced) ▾",
                        "Подключить облако (для продвинутых) ▾",
                    ));
                    toggle_btn.add_css_class("btn-secondary");
                    toggle_btn.set_valign(Align::Center);
                    top_row.append(&toggle_btn);

                    let defer_btn = Button::with_label(tr(lang, "Later", "Отложить"));
                    defer_btn.add_css_class("btn-secondary");
                    defer_btn.set_valign(Align::Center);
                    top_row.append(&defer_btn);
                    card.append(&top_row);

                    // Expandable guide & input form
                    let guide_box = Box::new(Orientation::Vertical, 8);
                    guide_box.set_visible(false);
                    guide_box.set_margin_top(6);

                    let guide_text = Label::new(Some(tr(
                        lang,
                        "ℹ Google Drive OAuth tokens expire after 7 days in testing mode. For a permanent token without re-login, click 'Publish App' (In Production) with scope 'drive.file' in Google Cloud Console. Alternatively, connect WebDAV (Yandex.Disk, Nextcloud) via 'rclone config'.",
                        "ℹ OAuth-токен Google Drive в тестовом режиме истекает через 7 дней. Для постоянного токена без повторной авторизации переведите проект в «Publish App» (In Production) со скоупом drive.file в Google Cloud Console. (Либо настройте WebDAV / Яндекс.Диск через команду rclone config).",
                    )));
                    guide_text.add_css_class("card-muted");
                    guide_text.set_wrap(true);
                    guide_text.set_xalign(0.0);
                    guide_box.append(&guide_text);

                    let creds_row = Box::new(Orientation::Horizontal, 8);
                    let id_entry = Entry::new();
                    id_entry.add_css_class("setup-entry");
                    id_entry.set_hexpand(true);
                    id_entry.set_placeholder_text(Some(tr(lang, "Client ID (optional)", "Client ID (по желанию)")));
                    creds_row.append(&id_entry);

                    let secret_entry = Entry::new();
                    secret_entry.add_css_class("setup-entry");
                    secret_entry.set_hexpand(true);
                    secret_entry.set_visibility(false);
                    secret_entry.set_placeholder_text(Some(tr(lang, "Client Secret (optional)", "Client Secret (по желанию)")));
                    creds_row.append(&secret_entry);
                    guide_box.append(&creds_row);

                    let action_row = Box::new(Orientation::Horizontal, 8);
                    let auth_btn = Button::with_label(tr(lang, "󰌆 Authorize in Browser", "󰌆 Авторизоваться в браузере"));
                    auth_btn.add_css_class("btn-primary");
                    action_row.append(&auth_btn);

                    let skip_btn = Button::with_label(tr(lang, "Skip (Don't ask again)", "Пропустить (больше не спрашивать)"));
                    skip_btn.add_css_class("btn-secondary");
                    action_row.append(&skip_btn);
                    guide_box.append(&action_row);
                    card.append(&guide_box);

                    {
                        let guide_box_c = guide_box.clone();
                        let toggle_btn_c = toggle_btn.clone();
                        toggle_btn.connect_clicked(move |_| {
                            let now_vis = !guide_box_c.is_visible();
                            guide_box_c.set_visible(now_vis);
                            toggle_btn_c.set_label(if now_vis {
                                tr(lang, "Hide Cloud Setup ▴", "Скрыть настройку ▴")
                            } else {
                                tr(lang, "Connect Cloud (Advanced) ▾", "Подключить облако (для продвинутых) ▾")
                            });
                        });
                    }

                    {
                        let deferred_ids_c = deferred_ids.clone();
                        let render_steps_c = render_steps_ref.clone();
                        let status_cb_c = status_cb.clone();
                        let step_id = step.id.clone();
                        let step_title = step.title.clone();
                        defer_btn.connect_clicked(move |_| {
                            deferred_ids_c.borrow_mut().insert(step_id.clone());
                            status_cb_c.warn(&match lang {
                                Lang::En => format!("Step '{step_title}' deferred"),
                                Lang::Ru => format!("Шаг «{step_title}» отложен до следующего входа"),
                            });
                            if let Some(ref f) = *render_steps_c.borrow() {
                                f(false);
                            }
                        });
                    }

                    {
                        let paths_c = paths.clone();
                        let render_steps_c = render_steps_ref.clone();
                        let status_cb_c = status_cb.clone();
                        let step_id = step.id.clone();
                        let step_title = step.title.clone();
                        skip_btn.connect_clicked(move |_| {
                            if let Err(e) = mark_step_done(&paths_c.first_login_state_dir(), &step_id) {
                                status_cb_c.error(&e);
                                return;
                            }
                            status_cb_c.ok(&match lang {
                                Lang::En => format!("Step '{step_title}' skipped"),
                                Lang::Ru => format!("Шаг «{step_title}» пропущен"),
                            });
                            if let Some(ref f) = *render_steps_c.borrow() {
                                f(true);
                            }
                        });
                    }

                    {
                        let step_c = step.clone();
                        let active_step = active_step.clone();
                        let terminal = terminal.clone();
                        let vte_box = vte_box.clone();
                        let vte_title = vte_title.clone();
                        let status_cb = status_cb.clone();
                        let id_entry = id_entry.clone();
                        let secret_entry = secret_entry.clone();

                        auth_btn.connect_clicked(move |_| {
                            if active_step.borrow().is_some() {
                                status_cb.warn(tr(
                                    lang,
                                    "Wait for the current terminal step to finish",
                                    "Дождитесь завершения текущего шага в терминале",
                                ));
                                return;
                            }
                            *active_step.borrow_mut() = Some(step_c.clone());
                            vte_title.set_text(&match lang {
                                Lang::En => format!("Running: {}", step_c.title),
                                Lang::Ru => format!("Выполняется: {}", step_c.title),
                            });
                            vte_box.set_visible(true);
                            terminal.reset(true, true);
                            terminal.grab_focus();

                            let step_str = step_c.path.to_string_lossy().to_string();
                            let cid = id_entry.text().trim().to_string();
                            let csec = secret_entry.text().trim().to_string();
                            let env_cid = format!("CLIENT_ID={cid}");
                            let env_csec = format!("CLIENT_SECRET={csec}");
                            let envv: Vec<&str> = vec![&env_cid, &env_csec];

                            terminal.spawn_async(
                                PtyFlags::DEFAULT,
                                None,
                                &[&step_str],
                                &envv,
                                glib::SpawnFlags::SEARCH_PATH,
                                || {},
                                -1,
                                Option::<&gio::Cancellable>::None,
                                |_| {},
                            );
                        });
                    }

                    list_box.append(&card);
                    continue;
                }

                let card = Box::new(Orientation::Horizontal, 12);
                card.add_css_class("setup-subcard");

                let info = Box::new(Orientation::Vertical, 2);
                info.set_hexpand(true);
                let title = Label::new(Some(&step.title));
                title.add_css_class("card-title");
                title.set_xalign(0.0);
                title.set_wrap(true);
                info.append(&title);

                let meta = Label::new(Some(&step.id));
                meta.add_css_class("card-muted");
                meta.set_xalign(0.0);
                info.append(&meta);
                card.append(&info);

                let pill = match step.status {
                    StepStatus::Configured => {
                        let p = Label::new(Some(tr(lang, "Configured", "Настроено")));
                        p.add_css_class("pill");
                        p.add_css_class("pill-ok");
                        p
                    }
                    StepStatus::Deferred => {
                        let p = Label::new(Some(tr(lang, "Deferred", "Отложено")));
                        p.add_css_class("pill");
                        p.add_css_class("pill-deferred");
                        p
                    }
                    StepStatus::Pending => {
                        let p = Label::new(Some(tr(lang, "Pending", "Ожидает")));
                        p.add_css_class("pill");
                        p.add_css_class("pill-pending");
                        p
                    }
                };
                pill.set_valign(Align::Center);
                card.append(&pill);

                let run_lbl = if step.status == StepStatus::Configured {
                    tr(lang, "Re-run", "Повторить")
                } else {
                    tr(lang, "Run", "Запустить")
                };
                let run_btn = Button::with_label(run_lbl);
                run_btn.add_css_class("btn-primary");
                run_btn.set_valign(Align::Center);
                card.append(&run_btn);

                if step.status == StepStatus::Pending {
                    let defer_btn = Button::with_label(tr(lang, "Later", "Отложить"));
                    defer_btn.add_css_class("btn-secondary");
                    defer_btn.set_valign(Align::Center);
                    defer_btn.set_tooltip_text(Some(tr(
                        lang,
                        "Postpone for this session",
                        "Напомнить при следующем входе",
                    )));
                    card.append(&defer_btn);

                    let skip_btn = Button::with_label(tr(lang, "Skip", "Пропустить"));
                    skip_btn.add_css_class("btn-secondary");
                    skip_btn.set_valign(Align::Center);
                    skip_btn.set_tooltip_text(Some(tr(
                        lang,
                        "Don't ask again (mark as done)",
                        "Больше не спрашивать (отметить выполненным)",
                    )));
                    card.append(&skip_btn);

                    let deferred_ids_c = deferred_ids.clone();
                    let render_steps_c = render_steps_ref.clone();
                    let status_cb_c = status_cb.clone();
                    let step_id = step.id.clone();
                    let step_title = step.title.clone();
                    defer_btn.connect_clicked(move |_| {
                        deferred_ids_c.borrow_mut().insert(step_id.clone());
                        status_cb_c.warn(&match lang {
                            Lang::En => format!("Step '{step_title}' deferred"),
                            Lang::Ru => format!("Шаг «{step_title}» отложен до следующего входа"),
                        });
                        if let Some(ref f) = *render_steps_c.borrow() {
                            f(false);
                        }
                    });

                    let paths_c = paths.clone();
                    let render_steps_c = render_steps_ref.clone();
                    let status_cb_c = status_cb.clone();
                    let step_id = step.id.clone();
                    let step_title = step.title.clone();
                    skip_btn.connect_clicked(move |_| {
                        if let Err(e) = mark_step_done(&paths_c.first_login_state_dir(), &step_id) {
                            status_cb_c.error(&e);
                            return;
                        }
                        status_cb_c.ok(&match lang {
                            Lang::En => format!("Step '{step_title}' skipped"),
                            Lang::Ru => format!("Шаг «{step_title}» пропущен"),
                        });
                        if let Some(ref f) = *render_steps_c.borrow() {
                            f(true);
                        }
                    });
                }

                // Wire Run button
                {
                    let step_c = step.clone();
                    let active_step = active_step.clone();
                    let terminal = terminal.clone();
                    let vte_box = vte_box.clone();
                    let vte_title = vte_title.clone();
                    let status_cb = status_cb.clone();
                    let paths = paths.clone();
                    let render_steps_c = render_steps_ref.clone();

                    run_btn.connect_clicked(move |_| {
                        if active_step.borrow().is_some() {
                            status_cb.warn(tr(
                                lang,
                                "Wait for the current terminal step to finish",
                                "Дождитесь завершения текущего шага в терминале",
                            ));
                            return;
                        }
                        match step_c.mode {
                            StepMode::Gui => {
                                let path = step_c.path.clone();
                                let step_id = step_c.id.clone();
                                let title = step_c.title.clone();
                                let state_dir = paths.first_login_state_dir();
                                let status_cb = status_cb.clone();
                                let render_steps_c = render_steps_c.clone();
                                status_cb.warn(&match lang {
                                    Lang::En => format!("Running '{title}'..."),
                                    Lang::Ru => format!("Запуск «{title}»..."),
                                });
                                glib::MainContext::default().spawn_local(async move {
                                    let path_bg = path.clone();
                                    let ok = gio::spawn_blocking(move || {
                                        Command::new(&path_bg)
                                            .status()
                                            .map(|s| s.success())
                                            .unwrap_or(false)
                                            || run_step_check(&path_bg)
                                    })
                                    .await
                                    .unwrap_or(false);
                                    if ok {
                                        let _ = mark_step_done(&state_dir, &step_id);
                                        status_cb.ok(&match lang {
                                            Lang::En => format!("Step '{title}' completed"),
                                            Lang::Ru => format!("Шаг «{title}» завершён"),
                                        });
                                    } else {
                                        status_cb.warn(&match lang {
                                            Lang::En => format!("Step '{title}' is not configured yet"),
                                            Lang::Ru => format!("Шаг «{title}» ещё не настроен"),
                                        });
                                    }
                                    if let Some(ref f) = *render_steps_c.borrow() {
                                        f(true);
                                    }
                                });
                            }
                            StepMode::Terminal => {
                                *active_step.borrow_mut() = Some(step_c.clone());
                                vte_title.set_text(&match lang {
                                    Lang::En => format!("Running: {}", step_c.title),
                                    Lang::Ru => format!("Выполняется: {}", step_c.title),
                                });
                                vte_box.set_visible(true);
                                terminal.reset(true, true);
                                terminal.grab_focus();

                                let step_str = step_c.path.to_string_lossy().to_string();
                                let active_step_err = active_step.clone();
                                let status_cb_err = status_cb.clone();
                                let step_title_err = step_c.title.clone();
                                let step_path_fallback = step_c.path.clone();

                                terminal.spawn_async(
                                    PtyFlags::DEFAULT,
                                    None,
                                    &[&step_str],
                                    &[],
                                    glib::SpawnFlags::DEFAULT,
                                    || {},
                                    -1,
                                    gio::Cancellable::NONE,
                                    move |res| {
                                        if let Err(e) = res {
                                            *active_step_err.borrow_mut() = None;
                                            status_cb_err.warn(&match lang {
                                                Lang::En => format!("VTE error ({e}), opening kitty..."),
                                                Lang::Ru => format!("VTE ошибка ({e}), открываю kitty..."),
                                            });
                                            let _ = Command::new("kitty")
                                                .args([
                                                    "--class",
                                                    "inerium-first-login",
                                                    "--title",
                                                    &step_title_err,
                                                ])
                                                .arg(&step_path_fallback)
                                                .spawn();
                                        }
                                    },
                                );
                            }
                        }
                    });
                }

                list_box.append(&card);
            }
        })
    };

    *render_steps.borrow_mut() = Some(render_fn.clone());
    render_fn(false);

    {
        let render_fn = render_fn.clone();
        let status_cb = status_cb.clone();
        recheck_btn.connect_clicked(move |_| {
            render_fn(true);
            status_cb.ok(tr(lang, "Step status refreshed", "Статус шагов обновлён"));
        });
    }

    root
}

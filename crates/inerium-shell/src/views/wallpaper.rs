use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{
    Box, Button, FileDialog, FileFilter, Label, Orientation, Picture, PolicyType, ScrolledWindow,
};

use crate::hw;
use inerium_i18n::{tr, tr_fmt};

type ActionCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

pub fn build_wallpaper_view<FNav, FClose>(
    nav_cb: FNav,
    _close_cb: FClose,
    current_monitor: Option<String>,
) -> (Box, i32, i32)
where
    FNav: Fn(&str) + 'static + Clone,
    FClose: Fn() + 'static + Clone,
{
    let root = Box::new(Orientation::Vertical, 10);
    root.add_css_class("popup-container");
    root.set_size_request(420, -1);

    // Header
    let head = Box::new(Orientation::Horizontal, 8);
    let back_btn = Button::with_label(tr("‹ Back", "‹ Назад"));
    back_btn.add_css_class("back-btn");
    let n_cb = nav_cb.clone();
    back_btn.connect_clicked(move |_| n_cb("control"));
    head.append(&back_btn);

    let title_box = Box::new(Orientation::Vertical, 2);
    title_box.set_hexpand(true);

    let title = Label::new(Some(tr("󰸉 Wallpaper Gallery", "󰸉 Галерея обоев")));
    title.add_css_class("cc-host");
    title.set_xalign(0.0);
    title_box.append(&title);

    let path_lbl = Label::new(Some("~/.local/share/inerium/wallpapers"));
    path_lbl.add_css_class("cc-meta");
    path_lbl.set_xalign(0.0);
    title_box.append(&path_lbl);

    head.append(&title_box);

    let add_btn = Button::with_label(tr("+ Add", "+ Добавить"));
    add_btn.add_css_class("btn-secondary");
    add_btn.set_tooltip_text(Some(tr("Add custom wallpapers from files", "Добавить свои обои из файлов")));
    head.append(&add_btn);
    root.append(&head);

    // Dynamic Multi-Monitor Selection
    let connected_monitors = hw::list_connected_monitors();
    let has_multiple_monitors = connected_monitors.len() > 1;

    let default_target = if let Some(ref cur) = current_monitor {
        if connected_monitors.iter().any(|m| &m.name == cur) {
            cur.clone()
        } else if has_multiple_monitors {
            "all".to_string()
        } else if let Some(first) = connected_monitors.first() {
            first.name.clone()
        } else {
            "all".to_string()
        }
    } else if has_multiple_monitors {
        "all".to_string()
    } else if let Some(first) = connected_monitors.first() {
        first.name.clone()
    } else {
        "all".to_string()
    };

    let selected_target = Rc::new(RefCell::new(default_target));
    let active_map = Rc::new(RefCell::new(hw::get_active_wallpapers()));

    let seg_buttons = Rc::new(RefCell::new(Vec::<(String, Button)>::new()));

    if has_multiple_monitors {
        let seg = Box::new(Orientation::Horizontal, 2);
        seg.add_css_class("cc-seg");
        seg.set_homogeneous(true);

        // "All Monitors" button
        let all_btn = Button::with_label(tr("󰍹 All", "󰍹 Все"));
        all_btn.set_tooltip_text(Some(tr("Apply to all monitors", "Применить ко всем мониторам сразу")));
        seg_buttons.borrow_mut().push(("all".to_string(), all_btn.clone()));
        seg.append(&all_btn);

        // Individual dynamic monitor buttons
        for m in &connected_monitors {
            let m_btn = Button::with_label(&m.label);
            m_btn.set_tooltip_text(Some(&tr_fmt!("Select monitor {name}", "Выбрать монитор {name}", name = m.name)));
            seg_buttons.borrow_mut().push((m.name.clone(), m_btn.clone()));
            seg.append(&m_btn);
        }

        root.append(&seg);
    }

    // Horizontal Scrolled Gallery
    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Never)
        .kinetic_scrolling(true)
        .overlay_scrolling(true)
        .hexpand(true)
        .build();
    scroll.set_size_request(-1, 150);

    let gallery_box = Box::new(Orientation::Horizontal, 12);
    gallery_box.set_margin_top(4);
    gallery_box.set_margin_bottom(4);
    gallery_box.set_margin_start(4);
    gallery_box.set_margin_end(4);

    let card_entries = Rc::new(RefCell::new(Vec::<(String, Button)>::new()));

    scroll.set_child(Some(&gallery_box));
    root.append(&scroll);

    // Tip / Target feedback at bottom
    let tip = Label::new(None);
    tip.add_css_class("cc-meta");
    tip.set_xalign(0.5);
    tip.set_margin_top(2);
    root.append(&tip);

    // Reactive UI updater closure
    let update_ui: ActionCallback = Rc::new(RefCell::new(None));
    {
        let selected_target = selected_target.clone();
        let active_map = active_map.clone();
        let seg_buttons = seg_buttons.clone();
        let card_entries = card_entries.clone();
        let tip = tip.clone();
        let connected_monitors = connected_monitors.clone();

        let u_fn: Rc<dyn Fn()> = Rc::new(move || {
            let target = selected_target.borrow().clone();

            // 1. Sync segmented tab buttons
            for (t, btn) in seg_buttons.borrow().iter() {
                if t == &target {
                    btn.add_css_class("active");
                } else {
                    btn.remove_css_class("active");
                }
            }

            // 2. Determine which wallpaper path is active for the current target
            let map = active_map.borrow();
            let active_path_for_target = if target == "all" {
                let mut common: Option<String> = None;
                let mut all_same = true;
                for m in &connected_monitors {
                    if let Some(p) = map.get(&m.name) {
                        match &common {
                            None => common = Some(p.clone()),
                            Some(prev) if prev != p => {
                                all_same = false;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                if all_same { common } else { None }
            } else {
                map.get(&target).cloned()
            };

            // 3. Sync wallpaper card highlights
            for (path, btn) in card_entries.borrow().iter() {
                if Some(path) == active_path_for_target.as_ref() {
                    btn.add_css_class("active");
                } else {
                    btn.remove_css_class("active");
                }
            }

            // 4. Update status / tip
            if connected_monitors.len() > 1 {
                if target == "all" {
                    tip.set_label(tr(
                        "All screens · Click a card to apply",
                        "Все экраны · Кликните на карточку для применения",
                    ));
                } else {
                    let label = connected_monitors
                        .iter()
                        .find(|m| m.name == target)
                        .map(|m| m.label.as_str())
                        .unwrap_or(&target);
                    tip.set_label(&tr_fmt!(
                        "Screen {label} · Click a card to apply",
                        "Экран {label} · Кликните на карточку для применения",
                        label = label,
                    ));
                }
            } else {
                tip.set_label(tr(
                    "Click a card to apply wallpaper instantly",
                    "Кликните на карточку для мгновенного применения обоев",
                ));
            }
        });
        *update_ui.borrow_mut() = Some(u_fn);
    }

    // Gallery builder closure (runs on start and when wallpapers are added/deleted)
    let rebuild_gallery: ActionCallback = Rc::new(RefCell::new(None));
    {
        let gallery_box = gallery_box.clone();
        let card_entries = card_entries.clone();
        let selected_target = selected_target.clone();
        let active_map = active_map.clone();
        let connected_monitors = connected_monitors.clone();
        let update_ui = update_ui.clone();
        let rebuild_ref = rebuild_gallery.clone();

        let r_fn: Rc<dyn Fn()> = Rc::new(move || {
            while let Some(child) = gallery_box.first_child() {
                gallery_box.remove(&child);
            }
            card_entries.borrow_mut().clear();

            let walls = hw::list_available_wallpapers();
            let inerium_dir = hw::inerium_wallpapers_dir();

            if walls.is_empty() {
                let empty_lbl = Label::new(Some(tr(
                    "No wallpapers found in wallpaper folders",
                    "Обои не найдены в папках обоев",
                )));
                empty_lbl.add_css_class("cc-meta");
                empty_lbl.set_margin_top(40);
                gallery_box.append(&empty_lbl);
            } else {
                for w in walls {
                    let card_btn = Button::new();
                    card_btn.add_css_class("wall-card");

                    let card_inner = Box::new(Orientation::Vertical, 4);

                    let img_src = w.thumb_path.as_deref().unwrap_or(&w.path);
                    let pic = Picture::for_filename(Path::new(img_src));
                    pic.set_content_fit(gtk4::ContentFit::Contain);
                    pic.set_can_shrink(true);
                    pic.set_size_request(140, 80);
                    pic.add_css_class("wall-thumb");
                    card_inner.append(&pic);

                    let cap_row = Box::new(Orientation::Horizontal, 4);

                    let mut display_name = w.name.clone();
                    if display_name.len() > 14 {
                        display_name.truncate(12);
                        display_name.push('…');
                    }
                    let name_lbl = Label::new(Some(&display_name));
                    name_lbl.add_css_class("wall-caption");
                    name_lbl.set_hexpand(true);
                    name_lbl.set_xalign(0.0);
                    cap_row.append(&name_lbl);

                    let is_custom = Path::new(&w.path).starts_with(&inerium_dir);
                    if is_custom {
                        let del_btn = Button::with_label("✕");
                        del_btn.add_css_class("btn-secondary");
                        del_btn.set_tooltip_text(Some(tr("Delete wallpaper", "Удалить обои")));
                        let p_del = w.path.clone();
                        let reb_self = rebuild_ref.clone();
                        let upd_self = update_ui.clone();
                        del_btn.connect_clicked(move |_| {
                            let _ = std::fs::remove_file(&p_del);
                            if let Some(ref rf) = *reb_self.borrow() {
                                rf();
                            }
                            if let Some(ref uf) = *upd_self.borrow() {
                                uf();
                            }
                        });
                        cap_row.append(&del_btn);
                    }

                    card_inner.append(&cap_row);
                    card_btn.set_child(Some(&card_inner));

                    let s_target = selected_target.clone();
                    let a_map = active_map.clone();
                    let u_ui = update_ui.clone();
                    let p = w.path.clone();
                    let c_mons = connected_monitors.clone();
                    card_btn.connect_clicked(move |_| {
                        let target = s_target.borrow().clone();
                        if target == "all" {
                            for m in &c_mons {
                                a_map.borrow_mut().insert(m.name.clone(), p.clone());
                            }
                            hw::set_wallpaper_async(p.clone(), None);
                        } else {
                            a_map.borrow_mut().insert(target.clone(), p.clone());
                            hw::set_wallpaper_async(p.clone(), Some(target.clone()));
                        }
                        if let Some(ref uf) = *u_ui.borrow() {
                            uf();
                        }
                    });

                    card_entries.borrow_mut().push((w.path.clone(), card_btn.clone()));
                    gallery_box.append(&card_btn);
                }
            }
        });
        *rebuild_gallery.borrow_mut() = Some(r_fn.clone());
        r_fn();
    }

    // Connect click handlers for monitor tabs
    for (t_name, btn) in seg_buttons.borrow().iter() {
        let s_target = selected_target.clone();
        let u_ui = update_ui.clone();
        let t = t_name.clone();
        btn.connect_clicked(move |_| {
            *s_target.borrow_mut() = t.clone();
            if let Some(ref uf) = *u_ui.borrow() {
                uf();
            }
        });
    }

    // Wire "+ Add Wallpaper" button
    {
        let rebuild_gallery = rebuild_gallery.clone();
        let update_ui = update_ui.clone();

        add_btn.connect_clicked(move |_| {
            let filter = FileFilter::new();
            filter.set_name(Some(tr(
                "Images (*.png, *.jpg, *.jpeg, *.webp)",
                "Изображения (*.png, *.jpg, *.jpeg, *.webp)",
            )));
            filter.add_mime_type("image/png");
            filter.add_mime_type("image/jpeg");
            filter.add_mime_type("image/webp");

            let filters = gio::ListStore::new::<FileFilter>();
            filters.append(&filter);

            let dialog = FileDialog::builder()
                .title(tr("Add Wallpapers", "Добавить обои"))
                .accept_label(tr("Add", "Добавить"))
                .filters(&filters)
                .modal(true)
                .build();

            crate::set_modal_dialog_active(true);

            let reb_c = rebuild_gallery.clone();
            let upd_c = update_ui.clone();

            dialog.open_multiple(
                Option::<&gtk4::Window>::None,
                gio::Cancellable::NONE,
                move |res| {
                    crate::set_modal_dialog_active(false);
                    if let Ok(files) = res {
                        let dest_dir = hw::inerium_wallpapers_dir();
                        let _ = std::fs::create_dir_all(&dest_dir);
                        let n = files.n_items();
                        for i in 0..n {
                            if let Some(item) = files.item(i)
                                && let Ok(file) = item.downcast::<gio::File>()
                                && let Some(src_path) = file.path()
                                && let Some(filename) = src_path.file_name()
                            {
                                let target = dest_dir.join(filename);
                                let _ = std::fs::copy(&src_path, &target);
                            }
                        }
                        if let Some(ref rf) = *reb_c.borrow() {
                            rf();
                        }
                        if let Some(ref uf) = *upd_c.borrow() {
                            uf();
                        }
                    }
                },
            );
        });
    }

    // Initial sync
    if let Some(ref uf) = *update_ui.borrow() {
        uf();
    }

    let req_h = if has_multiple_monitors { 295 } else { 265 };
    (root, 420, req_h)
}

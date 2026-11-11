use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Box, Button, Label, Orientation, Picture, PolicyType, ScrolledWindow};

use crate::hw;

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
    let back_btn = Button::with_label("‹ Назад");
    back_btn.add_css_class("back-btn");
    let n_cb = nav_cb.clone();
    back_btn.connect_clicked(move |_| n_cb("control"));
    head.append(&back_btn);

    let title_box = Box::new(Orientation::Vertical, 2);
    title_box.set_hexpand(true);

    let title = Label::new(Some("󰸉 Галерея обоев"));
    title.add_css_class("cc-host");
    title.set_xalign(0.0);
    title_box.append(&title);

    let path_lbl = Label::new(Some("~/Pictures/Wallpapers  ·  ~/Обои"));
    path_lbl.add_css_class("cc-meta");
    path_lbl.set_xalign(0.0);
    title_box.append(&path_lbl);

    head.append(&title_box);
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
        let all_btn = Button::with_label("󰍹 Все");
        all_btn.set_tooltip_text(Some("Применить ко всем мониторам сразу"));
        seg_buttons.borrow_mut().push(("all".to_string(), all_btn.clone()));
        seg.append(&all_btn);

        // Individual dynamic monitor buttons
        for m in &connected_monitors {
            let m_btn = Button::with_label(&m.label);
            m_btn.set_tooltip_text(Some(&format!("Выбрать монитор {}", m.name)));
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
    scroll.set_size_request(-1, 145);

    let gallery_box = Box::new(Orientation::Horizontal, 12);
    gallery_box.set_margin_top(4);
    gallery_box.set_margin_bottom(4);
    gallery_box.set_margin_start(4);
    gallery_box.set_margin_end(4);

    let walls = hw::list_available_wallpapers();
    let card_entries = Rc::new(RefCell::new(Vec::<(String, Button)>::new()));

    if walls.is_empty() {
        let empty_lbl = Label::new(Some("Обои не найдены в папках обоев"));
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

            let mut display_name = w.name.clone();
            if display_name.len() > 18 {
                display_name.truncate(16);
                display_name.push('…');
            }
            let name_lbl = Label::new(Some(&display_name));
            name_lbl.add_css_class("wall-caption");
            card_inner.append(&name_lbl);

            card_btn.set_child(Some(&card_inner));

            card_entries.borrow_mut().push((w.path.clone(), card_btn.clone()));
            gallery_box.append(&card_btn);
        }
    }

    scroll.set_child(Some(&gallery_box));
    root.append(&scroll);

    // Tip / Target feedback at bottom
    let tip = Label::new(None);
    tip.add_css_class("cc-meta");
    tip.set_xalign(0.5);
    tip.set_margin_top(2);
    root.append(&tip);

    // Reactive UI updater closure
    let update_ui = {
        let selected_target = selected_target.clone();
        let active_map = active_map.clone();
        let seg_buttons = seg_buttons.clone();
        let card_entries = card_entries.clone();
        let tip = tip.clone();
        let connected_monitors = connected_monitors.clone();

        Rc::new(move || {
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
                // If all connected monitors share the same wallpaper, highlight it
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
                if all_same {
                    common
                } else {
                    None
                }
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
                    tip.set_label("Все экраны · Кликните на карточку для применения");
                } else {
                    let label = connected_monitors
                        .iter()
                        .find(|m| m.name == target)
                        .map(|m| m.label.as_str())
                        .unwrap_or(&target);
                    tip.set_label(&format!("Экран {} · Кликните на карточку для применения", label));
                }
            } else {
                tip.set_label("Кликните на карточку для мгновенного применения обоев");
            }
        })
    };

    // Connect click handlers for monitor tabs
    for (t_name, btn) in seg_buttons.borrow().iter() {
        let s_target = selected_target.clone();
        let u_ui = update_ui.clone();
        let t = t_name.clone();
        btn.connect_clicked(move |_| {
            *s_target.borrow_mut() = t.clone();
            u_ui();
        });
    }

    // Connect click handlers for wallpaper cards
    for (card_path, card_btn) in card_entries.borrow().iter() {
        let s_target = selected_target.clone();
        let a_map = active_map.clone();
        let u_ui = update_ui.clone();
        let p = card_path.clone();
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
            u_ui();
        });
    }

    // Initial render
    update_ui();

    let req_h = if has_multiple_monitors { 290 } else { 260 };
    (root, 420, req_h)
}

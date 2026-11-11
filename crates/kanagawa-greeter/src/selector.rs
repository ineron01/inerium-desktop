use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone)]
pub struct SessionSelector {
    container: gtk4::Box,
    btn: gtk4::Button,
    btn_label: gtk4::Label,
    arrow: gtk4::Label,
    revealer: gtk4::Revealer,
    items: Vec<(gtk4::Button, gtk4::Label)>,
    selected_idx: Rc<Cell<usize>>,
    names: Vec<String>,
}

impl SessionSelector {
    pub fn new<F>(names: Vec<String>, initial_idx: usize, on_change: F) -> Self
    where
        F: Fn(usize) + 'static,
    {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 4);

        let initial = if !names.is_empty() && initial_idx < names.len() {
            initial_idx
        } else {
            0
        };
        let selected_idx = Rc::new(Cell::new(initial));

        let btn = gtk4::Button::new();
        btn.add_css_class("session-btn");

        let btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let initial_label = names.get(initial).cloned().unwrap_or_default();
        let btn_label = gtk4::Label::new(Some(&initial_label));
        btn_label.set_hexpand(true);
        btn_label.set_xalign(0.0);

        let arrow = gtk4::Label::new(Some("󰅀"));
        arrow.add_css_class("arrow");

        btn_box.append(&btn_label);
        btn_box.append(&arrow);
        btn.set_child(Some(&btn_box));
        container.append(&btn);

        let revealer = gtk4::Revealer::new();
        revealer.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        revealer.set_transition_duration(150);

        let menu_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        menu_box.add_css_class("session-menu");

        let mut items = Vec::new();
        let on_change = Rc::new(on_change);

        for (idx, name) in names.iter().enumerate() {
            let item_btn = gtk4::Button::new();
            item_btn.add_css_class("session-item");

            let item_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            let lbl = gtk4::Label::new(Some(name));
            lbl.set_hexpand(true);
            lbl.set_xalign(0.0);

            let check = gtk4::Label::new(Some(if idx == initial { "󰄬" } else { "" }));
            check.add_css_class("check");

            item_box.append(&lbl);
            item_box.append(&check);
            item_btn.set_child(Some(&item_box));

            if idx == initial {
                item_btn.add_css_class("selected");
            }

            menu_box.append(&item_btn);
            items.push((item_btn, check));
        }

        revealer.set_child(Some(&menu_box));
        container.append(&revealer);

        let selector = Self {
            container,
            btn: btn.clone(),
            btn_label,
            arrow: arrow.clone(),
            revealer: revealer.clone(),
            items,
            selected_idx: selected_idx.clone(),
            names,
        };

        // Клик по кнопке раскрывает / закрывает меню
        let sel_toggle = selector.clone();
        btn.connect_clicked(move |_| {
            sel_toggle.toggle_open();
        });

        // Клики по элементам списка
        for (idx, (item_btn, _)) in selector.items.iter().enumerate() {
            let sel_item = selector.clone();
            let on_change_clone = on_change.clone();
            item_btn.connect_clicked(move |_| {
                sel_item.select(idx, true);
                on_change_clone(idx);
            });
        }

        selector
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    pub fn toggle_open(&self) {
        let opened = !self.revealer.reveals_child();
        self.revealer.set_reveal_child(opened);
        self.arrow.set_label(if opened { "󰅃" } else { "󰅀" });
    }

    pub fn close(&self) {
        self.revealer.set_reveal_child(false);
        self.arrow.set_label("󰅀");
    }

    pub fn select(&self, idx: usize, _notify: bool) {
        if idx >= self.names.len() {
            return;
        }
        self.selected_idx.set(idx);
        self.btn_label.set_label(&self.names[idx]);

        for (i, (btn, check)) in self.items.iter().enumerate() {
            if i == idx {
                btn.add_css_class("selected");
                check.set_label("󰄬");
            } else {
                btn.remove_css_class("selected");
                check.set_label("");
            }
        }
        self.close();
    }

    pub fn get_selected(&self) -> usize {
        self.selected_idx.get()
    }

    pub fn set_selected(&self, idx: usize) {
        self.select(idx, false);
    }

    pub fn set_sensitive(&self, sensitive: bool) {
        self.btn.set_sensitive(sensitive);
        self.revealer.set_sensitive(sensitive);
        if !sensitive {
            self.close();
        }
    }
}

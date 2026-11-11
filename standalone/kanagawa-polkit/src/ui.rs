use gtk::prelude::*;
use gtk4::{
    glib::{clone, spawn_future_local},
    prelude::OrientableExt,
};
use relm4::prelude::*;
use tokio::sync::mpsc;

use crate::events::{AuthenticationAgentEvent, AuthenticationUserEvent};

pub const KANAGAWA_CSS: &str = r#"
window.kanagawa-polkit-window {
    background-color: transparent;
}

.kanagawa-card {
    background-color: rgba(24, 22, 22, 0.97);
    border: 1px solid rgba(197, 201, 197, 0.12);
    border-radius: 16px;
    padding: 22px 24px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.85);
    font-family: "JetBrainsMono Nerd Font", monospace;
    color: #c5c9c5;
}

.polkit-icon {
    color: #87a987;
    font-size: 26px;
    margin-right: 6px;
}

.polkit-title {
    font-size: 14px;
    font-weight: 700;
    color: #e6e2cc;
}

.polkit-subtitle {
    font-size: 11px;
    color: #737c73;
}

.polkit-message-box {
    background-color: #221f1f;
    border: 1px solid rgba(197, 201, 197, 0.08);
    border-radius: 10px;
    padding: 10px 14px;
    margin-top: 14px;
    margin-bottom: 12px;
}

.polkit-message {
    font-size: 12px;
    color: #c5c9c5;
    line-height: 1.35;
}

.polkit-error {
    font-size: 11px;
    color: #c4746e;
    font-weight: 600;
    margin-bottom: 8px;
}

.kanagawa-input {
    background-color: #221f1f;
    border: 1px solid rgba(197, 201, 197, 0.12);
    border-radius: 10px;
    color: #e6e2cc;
    padding: 8px 12px;
    font-size: 13px;
    font-family: "JetBrainsMono Nerd Font", monospace;
    min-height: 38px;
}

.kanagawa-input:focus-within {
    border-color: #87a987;
    box-shadow: 0 0 0 1px #87a987;
}

.kanagawa-dropdown {
    background-color: #221f1f;
    border: 1px solid rgba(197, 201, 197, 0.10);
    border-radius: 8px;
    color: #c5c9c5;
    font-size: 12px;
    margin-bottom: 10px;
}

.btn-cancel {
    background-color: rgba(197, 201, 197, 0.06);
    border: 1px solid rgba(197, 201, 197, 0.10);
    border-radius: 10px;
    color: #c5c9c5;
    font-size: 12px;
    font-weight: 600;
    padding: 8px 20px;
    min-height: 36px;
    min-width: 90px;
    transition: all 120ms ease;
}

.btn-cancel:hover {
    background-color: rgba(197, 201, 197, 0.12);
    color: #e6e2cc;
}

.btn-confirm {
    background-color: #87a987;
    border: 1px solid #87a987;
    border-radius: 10px;
    color: #181616;
    font-size: 12px;
    font-weight: 700;
    padding: 8px 22px;
    min-height: 36px;
    min-width: 110px;
    transition: all 120ms ease;
}

.btn-confirm:hover {
    background-color: #98bb98;
    border-color: #98bb98;
}

.btn-confirm:disabled {
    background-color: rgba(135, 169, 135, 0.35);
    color: rgba(24, 22, 22, 0.6);
    border-color: transparent;
}
"#;

#[derive(Debug, zeroize::ZeroizeOnDrop)]
pub enum AppMsg {
    Confirm { user: String, password: String },
    Cancel,
    AuthEvent(AuthenticationAgentEvent),
}

pub struct App {
    message: String,
    identities: Vec<String>,
    cookie: Option<String>,
    retry_message: Option<String>,
    authenticating: bool,
    sender: mpsc::Sender<AuthenticationUserEvent>,
}

#[allow(unused_assignments)]
#[relm4::component(async, pub)]
impl AsyncComponent for App {
    type Input = AppMsg;
    type Output = ();
    type Init = (
        mpsc::Sender<AuthenticationUserEvent>,
        mpsc::Receiver<AuthenticationAgentEvent>,
    );
    type CommandOutput = ();

    view! {
        gtk::Window {
            set_title: Some("Подтверждение действия"),
            set_default_width: 440,
            set_resizable: false,
            set_modal: false,
            set_hide_on_close: true,
            add_css_class: "kanagawa-polkit-window",
            #[watch]
            set_visible: model.cookie.is_some(),

            connect_show[password_entry] => move |_| {
                password_entry.grab_focus();
            },
            connect_close_request[cancel_button] => move |_| {
                cancel_button.emit_clicked();
                gtk4::glib::Propagation::Proceed
            },

            add_controller = gtk::EventControllerKey {
                connect_key_pressed[sender] => move |_, key, _, _| {
                    if key == gtk::gdk::Key::Escape {
                        sender.input(AppMsg::Cancel);
                        gtk4::glib::Propagation::Stop
                    } else {
                        gtk4::glib::Propagation::Proceed
                    }
                }
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "kanagawa-card",

                // Шапка карточки
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,
                    set_margin_bottom: 4,

                    gtk::Label {
                        set_label: "󰌾",
                        add_css_class: "polkit-icon",
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_valign: gtk::Align::Center,

                        gtk::Label {
                            set_label: "Подтверждение действия",
                            set_xalign: 0.0,
                            add_css_class: "polkit-title",
                        },

                        gtk::Label {
                            set_label: "Повышение привилегий суперпользователя",
                            set_xalign: 0.0,
                            add_css_class: "polkit-subtitle",
                        },
                    },
                },

                // Блок с сообщением действия
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    add_css_class: "polkit-message-box",

                    gtk::Label {
                        #[watch]
                        set_label: &model.message,
                        set_wrap: true,
                        set_xalign: 0.0,
                        add_css_class: "polkit-message",
                    },
                },

                // Сообщение об ошибке (при неверном пароле)
                gtk::Label {
                    #[watch]
                    set_label: &if let Some(retry) = model.retry_message.clone() {
                        format!("󰅚 {}", retry)
                    } else {
                        "".to_string()
                    },
                    #[watch]
                    set_visible: model.retry_message.is_some(),
                    set_xalign: 0.0,
                    add_css_class: "polkit-error",
                },

                // Селектор пользователя (если больше одного)
                #[name = "identity_dropdown"]
                gtk::DropDown {
                    add_css_class: "kanagawa-dropdown",
                    #[watch]
                    set_visible: model.identities.len() > 1,
                    #[watch]
                    set_model: Some(&gtk::StringList::new(&model.identities.iter().map(AsRef::as_ref).collect::<Vec<_>>())),
                    #[watch]
                    set_selected: {
                        if let Some(Ok(user)) = etc_passwd::Passwd::current_user().ok().flatten().map(|p| p.name.into_string()) {
                            model.identities.iter().position(|n| n == &user).unwrap_or(0) as u32
                        } else {
                            0
                        }
                    },
                },

                // Поле ввода пароля
                #[name = "password_entry"]
                gtk::PasswordEntry {
                    set_hexpand: true,
                    set_placeholder_text: Some("Введите пароль администратора..."),
                    set_show_peek_icon: true,
                    add_css_class: "kanagawa-input",
                    #[watch]
                    set_editable: !model.authenticating,

                    connect_activate[confirm_button] => move |_| {
                        confirm_button.emit_clicked();
                    }
                },

                // Панель кнопок (Отмена / Подтвердить)
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_halign: gtk::Align::End,
                    set_spacing: 12,
                    set_margin_top: 18,

                    #[name = "cancel_button"]
                    gtk::Button {
                        set_label: "Отмена",
                        add_css_class: "btn-cancel",
                        connect_clicked[sender, password_entry] => move |_| {
                            sender.input(AppMsg::Cancel);
                            password_entry.set_text("");
                        }
                    },

                    #[name = "confirm_button"]
                    gtk::Button {
                        set_label: "Подтвердить",
                        add_css_class: "btn-confirm",
                        #[watch]
                        set_sensitive: !model.authenticating,
                        connect_clicked[sender, identity_dropdown, password_entry] => move |_| {
                            let user_str = if let Some(item) = identity_dropdown.selected_item() {
                                if let Ok(obj) = item.dynamic_cast::<gtk::StringObject>() {
                                    obj.string().to_string()
                                } else {
                                    whoami_user()
                                }
                            } else {
                                whoami_user()
                            };

                            sender.input(AppMsg::Confirm {
                                user: user_str,
                                password: password_entry.text().to_string(),
                            });
                            password_entry.set_text("");
                        }
                    },
                },
            }
        }
    }

    async fn init(
        init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let model = App {
            message: String::new(),
            identities: Vec::new(),
            sender: init.0,
            cookie: None,
            authenticating: false,
            retry_message: None,
        };

        spawn_future_local(clone!(
            #[strong]
            sender,
            async move {
                let mut receiver = init.1;
                loop {
                    let event = receiver.recv().await.expect("Channel closed");
                    tracing::debug!("received event {:#?}", event);
                    sender.input(AppMsg::AuthEvent(event));
                }
            }
        ));

        let widgets = view_output!();

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        message: Self::Input,
        _sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match &message {
            AppMsg::Confirm { user, password } => {
                if let Some(cookie) = self.cookie.clone() {
                    self.sender
                        .send(AuthenticationUserEvent::ProvidedPassword {
                            cookie,
                            username: user.clone(),
                            password: password.clone(),
                        })
                        .await
                        .unwrap();
                    self.retry_message = Some("Проверка пароля...".to_string());
                    self.authenticating = true;
                }
            }
            AppMsg::Cancel => {
                if let Some(cookie) = self.cookie.clone() {
                    self.sender
                        .send(AuthenticationUserEvent::Canceled { cookie })
                        .await
                        .unwrap();
                    self.cookie = None;
                    self.message.clear();
                    self.retry_message = None;
                    self.authenticating = false;
                    self.identities.clear();
                }
            }
            AppMsg::AuthEvent(ev) => match ev {
                AuthenticationAgentEvent::Started {
                    cookie,
                    message,
                    names,
                } => {
                    if self.cookie.is_none() {
                        self.cookie = Some(cookie.clone());
                        self.message = message.clone();
                        self.identities = names.clone();
                        self.authenticating = false;
                        self.retry_message = None;
                    }
                }
                AuthenticationAgentEvent::Canceled { cookie } => {
                    if let Some(c) = &self.cookie {
                        if c == cookie {
                            self.cookie = None;
                            self.message.clear();
                            self.identities.clear();
                            self.retry_message = None;
                            self.authenticating = false;
                        }
                    }
                }
                AuthenticationAgentEvent::AuthorizationSucceeded { cookie } => {
                    if let Some(c) = &self.cookie {
                        if c == cookie {
                            tracing::debug!("Authorization succeeded, closing dialog.");
                            self.cookie = None;
                            self.message.clear();
                            self.identities.clear();
                            self.retry_message = None;
                            self.authenticating = false;
                        }
                    }
                }
                AuthenticationAgentEvent::AuthorizationRetry {
                    cookie,
                    retry_message,
                } => {
                    if let Some(c) = &self.cookie {
                        if c == cookie {
                            self.retry_message = retry_message.clone().or_else(|| Some("Неверный пароль. Повторите попытку.".to_string()));
                            self.authenticating = false;
                        }
                    }
                }
            },
        }
    }
}

fn whoami_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "root".to_string())
}

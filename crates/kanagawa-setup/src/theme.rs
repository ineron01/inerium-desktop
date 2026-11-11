use gdk4::RGBA;

#[allow(dead_code)]
pub mod palette {
    pub const BG: &str = "#181616";
    pub const SURFACE: &str = "#221f1f";
    pub const SURFACE_ALT: &str = "#282727";
    pub const FG: &str = "#c5c9c5";
    pub const FG_BRIGHT: &str = "#e6e2cc";
    pub const MUTED: &str = "#737c73";
    pub const SUBTLE: &str = "#a6a69c";
    pub const GREEN: &str = "#87a987";
    pub const BLUE: &str = "#8ba4b0";
    pub const AQUA: &str = "#8ea4a2";
    pub const YELLOW: &str = "#c4b28a";
    pub const ORANGE: &str = "#b6927b";
    pub const RED: &str = "#c4746e";
    pub const VIOLET: &str = "#8992a7";
}

pub fn vte_palette() -> (RGBA, RGBA, [RGBA; 16]) {
    let parse = |hex: &str| RGBA::parse(hex).unwrap_or(RGBA::BLACK);
    let fg = parse(palette::FG);
    let bg = parse("#141212");
    let pal = [
        parse("#181616"), // 0 black
        parse(palette::RED),
        parse(palette::GREEN),
        parse(palette::YELLOW),
        parse(palette::BLUE),
        parse(palette::VIOLET),
        parse(palette::AQUA),
        parse(palette::FG),
        parse(palette::MUTED),
        parse("#d48680"),
        parse("#9ab89a"),
        parse("#d4c49c"),
        parse("#9cb4c0"),
        parse("#9ba4b8"),
        parse("#9eb4b2"),
        parse(palette::FG_BRIGHT),
    ];
    (fg, bg, pal)
}

pub const SETUP_CSS: &str = r#"
@define-color bg          #181616;
@define-color surface     #221f1f;
@define-color surface-alt #282727;
@define-color line        rgba(197, 201, 197, 0.12);
@define-color hover       rgba(197, 201, 197, 0.08);
@define-color fg          #c5c9c5;
@define-color fg-bright   #e6e2cc;
@define-color muted       #737c73;
@define-color subtle      #a6a69c;
@define-color green       #87a987;
@define-color blue        #8ba4b0;
@define-color aqua        #8ea4a2;
@define-color yellow      #c4b28a;
@define-color orange      #b6927b;
@define-color red         #c4746e;

window.setup-window {
    background-color: #181616;
    color: #c5c9c5;
    font-family: "JetBrainsMono Nerd Font", monospace;
}

.setup-shell {
    background-color: #181616;
    color: #c5c9c5;
    padding: 18px 20px 16px 20px;
}

.setup-header {
    padding-bottom: 12px;
    border-bottom: 1px solid rgba(197, 201, 197, 0.10);
}

.setup-brand-ico {
    min-width: 38px;
    min-height: 38px;
    border-radius: 11px;
    background-color: rgba(135, 169, 135, 0.18);
    border: 1px solid rgba(135, 169, 135, 0.35);
    color: #87a987;
    font-size: 18px;
}

.setup-title {
    font-size: 16px;
    font-weight: 800;
    color: #e6e2cc;
}

.setup-subtitle {
    font-size: 11.5px;
    color: #737c73;
}

/* Navigation Tabs */
.setup-tabs {
    background-color: #221f1f;
    border: 1px solid rgba(197, 201, 197, 0.09);
    border-radius: 12px;
    padding: 4px;
}

.setup-tab-btn {
    min-height: 34px;
    padding: 6px 14px;
    border-radius: 9px;
    border: none;
    background-image: none;
    background-color: transparent;
    box-shadow: none;
    color: #a6a69c;
    font-size: 12px;
    font-weight: 600;
    transition: background-color 140ms ease, color 140ms ease;
}

.setup-tab-btn:hover {
    background-image: none;
    background-color: rgba(197, 201, 197, 0.08);
    color: #e6e2cc;
}

.setup-tab-btn.active {
    background-image: none;
    background-color: rgba(135, 169, 135, 0.22);
    color: #e6e2cc;
}

.setup-badge {
    min-width: 18px;
    min-height: 18px;
    padding: 0 6px;
    border-radius: 99px;
    background-color: rgba(196, 178, 138, 0.22);
    color: #c4b28a;
    font-size: 10.5px;
    font-weight: 700;
}

/* Cards & Sections */
.setup-card {
    background-color: #221f1f;
    border: 1px solid rgba(197, 201, 197, 0.09);
    border-radius: 14px;
    padding: 14px 16px;
}

.setup-subcard {
    background-color: #1c1a1a;
    border: 1px solid rgba(197, 201, 197, 0.08);
    border-radius: 12px;
    padding: 12px 14px;
}

.section-label {
    font-size: 11px;
    font-weight: 700;
    color: #737c73;
    letter-spacing: 0.8px;
}

.card-title {
    font-size: 13.5px;
    font-weight: 700;
    color: #e6e2cc;
}

.card-meta {
    font-size: 11.5px;
    color: #a6a69c;
}

.card-muted {
    font-size: 11px;
    color: #737c73;
}

/* Buttons */
.btn-primary {
    min-height: 34px;
    padding: 6px 14px;
    border-radius: 10px;
    border: 1px solid rgba(135, 169, 135, 0.45);
    background-image: none;
    background-color: #87a987;
    box-shadow: none;
    color: #181616;
    font-size: 12px;
    font-weight: 700;
}
.btn-primary label {
    color: #181616;
}
.btn-primary:hover {
    background-image: none;
    background-color: #98ba98;
}
.btn-primary:disabled {
    opacity: 0.55;
}

.btn-secondary {
    min-height: 34px;
    padding: 6px 14px;
    border-radius: 10px;
    border: 1px solid rgba(197, 201, 197, 0.14);
    background-image: none;
    background-color: #282727;
    box-shadow: none;
    color: #c5c9c5;
    font-size: 12px;
    font-weight: 600;
}
.btn-secondary:hover {
    background-image: none;
    background-color: rgba(197, 201, 197, 0.12);
    color: #e6e2cc;
}

.btn-danger {
    min-height: 34px;
    padding: 6px 14px;
    border-radius: 10px;
    border: 1px solid rgba(196, 116, 110, 0.35);
    background-image: none;
    background-color: rgba(196, 116, 110, 0.12);
    box-shadow: none;
    color: #c4746e;
    font-size: 12px;
    font-weight: 600;
}
.btn-danger label {
    color: #c4746e;
}
.btn-danger:hover {
    background-image: none;
    background-color: rgba(196, 116, 110, 0.24);
    color: #e6e2cc;
}
.btn-danger:hover label {
    color: #e6e2cc;
}

.seg-group {
    background-color: #181616;
    border: 1px solid rgba(197, 201, 197, 0.10);
    border-radius: 10px;
    padding: 3px;
}

.seg-btn {
    min-height: 28px;
    min-width: 30px;
    padding: 4px 10px;
    border-radius: 8px;
    border: none;
    background-image: none;
    background-color: transparent;
    box-shadow: none;
    color: #a6a69c;
    font-size: 12px;
    font-weight: 600;
}
.seg-btn:hover {
    background-image: none;
    background-color: rgba(197, 201, 197, 0.08);
    color: #e6e2cc;
}
.seg-btn.active {
    background-image: none;
    background-color: rgba(135, 169, 135, 0.24);
    color: #e6e2cc;
}

/* Inputs & Dropdowns */
entry.setup-entry {
    min-height: 34px;
    padding: 0 10px;
    border-radius: 9px;
    background-color: #181616;
    background-image: none;
    color: #e6e2cc;
    border: 1px solid rgba(197, 201, 197, 0.14);
    box-shadow: none;
    font-size: 12px;
}
entry.setup-entry:focus-within {
    border-color: rgba(135, 169, 135, 0.65);
}

dropdown.setup-dropdown > button {
    min-height: 34px;
    padding: 4px 10px;
    border-radius: 9px;
    background-color: #181616;
    background-image: none;
    color: #e6e2cc;
    border: 1px solid rgba(197, 201, 197, 0.14);
    box-shadow: none;
    font-size: 12px;
}
dropdown.setup-dropdown > button:hover {
    background-image: none;
    border-color: rgba(197, 201, 197, 0.28);
}

popover.background > contents {
    background-color: #221f1f;
    border: 1px solid rgba(197, 201, 197, 0.16);
    border-radius: 10px;
    color: #c5c9c5;
}

switch.setup-switch {
    background-image: none;
    background-color: #282727;
    border: 1px solid rgba(197, 201, 197, 0.16);
    border-radius: 99px;
    min-width: 44px;
    min-height: 24px;
}
switch.setup-switch:checked {
    background-image: none;
    background-color: rgba(135, 169, 135, 0.35);
    border-color: rgba(135, 169, 135, 0.65);
}
switch.setup-switch slider {
    background-image: none;
    background-color: #e6e2cc;
    border-radius: 99px;
    min-width: 18px;
    min-height: 18px;
}

/* Wallpaper Cards */
.wall-card {
    background-color: #181616;
    background-image: none;
    border: 1px solid rgba(197, 201, 197, 0.10);
    border-radius: 12px;
    padding: 6px;
    box-shadow: none;
}
.wall-card:hover {
    background-image: none;
    background-color: #282727;
    border-color: rgba(135, 169, 135, 0.45);
}
.wall-card.active {
    background-image: none;
    border: 2px solid #87a987;
    background-color: rgba(135, 169, 135, 0.12);
}
.wall-thumb {
    border-radius: 8px;
}
.wall-name {
    font-size: 10.5px;
    color: #c5c9c5;
}

/* Monitor Layout Preview */
.layout-preview {
    background-color: #181616;
    border: 1px solid rgba(197, 201, 197, 0.10);
    border-radius: 10px;
    padding: 8px 12px;
}
.mon-box {
    background-color: rgba(139, 164, 176, 0.14);
    border: 1px solid rgba(139, 164, 176, 0.45);
    border-radius: 8px;
    padding: 6px 10px;
}
.mon-box.focused {
    background-color: rgba(135, 169, 135, 0.18);
    border-color: rgba(135, 169, 135, 0.60);
}

/* Status Pills */
.pill {
    padding: 3px 9px;
    border-radius: 99px;
    font-size: 10.5px;
    font-weight: 700;
}
.pill-ok {
    background-color: rgba(135, 169, 135, 0.18);
    border: 1px solid rgba(135, 169, 135, 0.40);
    color: #87a987;
}
.pill-pending {
    background-color: rgba(196, 178, 138, 0.18);
    border: 1px solid rgba(196, 178, 138, 0.40);
    color: #c4b28a;
}
.pill-deferred {
    background-color: rgba(197, 201, 197, 0.08);
    border: 1px solid rgba(197, 201, 197, 0.16);
    color: #a6a69c;
}

/* VTE Terminal Drawer */
.vte-frame {
    background-color: #141212;
    border: 1px solid rgba(135, 169, 135, 0.35);
    border-radius: 12px;
    padding: 10px;
}

/* Footer & Toast */
.setup-footer {
    padding-top: 10px;
    border-top: 1px solid rgba(197, 201, 197, 0.10);
}
.status-msg {
    font-size: 11.5px;
    color: #c4b28a;
}
.status-msg.ok {
    color: #87a987;
}
.status-msg.err {
    color: #c4746e;
}
"#;

//! The VCraft screen (`VCraftScreen`): sidebar with friends, the community
//! and groups; the friends page, chats, group creation, the call and
//! settings; menus, modals and the addons' pages. Drawn immediate-mode each
//! frame into a draw list, with the click targets it drew kept for the next
//! click.

use crate::addons::{self, AddonAction};
use crate::crypto::Trust;
use crate::gfx::{self, Draw, Gfx, Icon};
use crate::i18n::{t, tf};
use crate::relay::Status;
use crate::social::RequestKind;
use crate::state::{self, Group, LOBBY, Profile, Reply, ServerInvite};
use crate::util::now_ms;
use crate::{Core, Effect};
use std::collections::HashMap;

const PIN: Icon = ["...###...", "..#####..", "..#####..", "..#####..", ".#######.", "....#....", "....#....", "....#....", "....#...."];
const BELL_OFF: Icon = ["#...#....", ".#.###...", "..#####..", ".#.#####.", ".##.####.", ".###.###.", "#####.###", "....#..#.", "........#"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Friends,
    Chat,
    NewGroup,
    Call,
    Settings,
    Addon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modal {
    RenameGroup,
    Safety,
    JoinServer,
    DeleteAccount,
    /// Your nickname and note for a friend.
    FriendNote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slider {
    Threshold,
    MicVolume,
    OutputVolume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    AutoConnect,
    Notifications,
    MessageSound,
    ShareListening,
    NoiseSuppression,
    EchoCancellation,
    AutoMutePaused,
    AutoMuteUnfocused,
    AutoRejoin,
    ShareServer,
    HudCompact,
    HudUnread,
    DebugLog,
}

#[derive(Clone, Debug)]
pub enum Action {
    Go(Page),
    OpenChat(String),
    ConvMenu(String),
    StatusMenu(i32, i32),
    SetStatus(String),
    ToggleMute,
    ToggleDeafen,
    Relay(&'static str, Option<&'static str>, Option<String>),
    AcceptInvite(String),
    DeclineInvite(String),
    ReconnectVoice,
    FriendsTab(usize),
    SendFriendRequest,
    Kofi,
    Call(String),
    CallGroup(String),
    InviteToServer(String),
    OpenModal(Modal, Option<String>),
    CloseModal,
    ToggleMembers,
    SendComposer,
    SendSong(String),
    ClearReply,
    SetReply(Reply),
    DeleteMessage(String, String),
    LobbyUserMenu(String, String),
    MemberMenu(String, String),
    AddToGroupMenu(String),
    ToggleNewGroupMember(String),
    NewGroup,
    CreateGroup,
    ToggleUserMute(String),
    GroupAction(&'static str, String, Option<&'static str>, Option<String>),
    LobbyMute(String, bool),
    SaveRelay,
    SetToggle(Toggle),
    SetPushToTalk(bool),
    Slide(Slider),
    MicTest,
    CycleRingtone,
    PreviewRingtone,
    ExportData,
    PendingJoin(ServerInvite, Option<String>),
    Join(ServerInvite),
    DeleteAccount,
    TrustNewKey(String),
    SaveRename,
    BlockedTab,
    Focus(String),
    Addon(AddonAction),
    OpenAddonPage(String, Option<String>),
    AskToJoin(String),
    AcceptRequest(String, RequestKind),
    DismissRequest(String, RequestKind),
    TogglePin(String, String),
    Unpin(String, String),
    TogglePins,
    ToggleGroupMute(String),
    CopyGroupCode(String),
    JoinWithCode,
    SaveNote,
    CycleEffect(bool),
    CycleLanguage(bool),
    OpenLogFolder,
    None,
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub rect: [i32; 4],
    pub left: Action,
    pub right: Option<Action>,
}

#[derive(Clone, Debug)]
pub struct MenuItem {
    label: String,
    color: u32,
    action: Option<Action>,
}

#[derive(Clone, Debug, Default)]
pub struct Field {
    pub value: String,
    pub max: usize,
}

#[derive(Default)]
pub struct Ui {
    pub page: Page,
    pub conv: Option<String>,
    pub addon_page: Option<String>,
    pub friends_tab: usize,
    pub hide_members: bool,
    /// The pinned messages panel over the chat.
    pub show_pins: bool,
    pub fields: HashMap<String, Field>,
    pub focused: Option<String>,
    hits: Vec<Hit>,
    overlay_hits: Vec<Hit>,
    menu: Option<(Vec<MenuItem>, i32, i32)>,
    modal: Option<Modal>,
    modal_subject: Option<String>,
    pending_join: Option<ServerInvite>,
    pub reply_to: Option<Reply>,
    new_group_members: Vec<String>,
    dragging: Option<(Slider, i32, i32)>,
    pub mouse: [i32; 2],
    sidebar_scroll: i32,
    chat_scroll: i32,
    pub page_scroll: i32,
    sidebar_max: i32,
    chat_max: i32,
    page_max: i32,
    sb: [i32; 4],
    chat: [i32; 4],
    pub page_area: [i32; 4],
    previewing: bool,
    pub size: [i32; 2],
    pub open: bool,
    pub close_requested: bool,
}

/// The drawing context of one frame: the draw list and the click targets.
pub struct Ctx<'a> {
    pub g: Gfx<'a>,
    pub hits: Vec<Hit>,
    pub mouse: [i32; 2],
    pub tooltip: Option<String>,
    pub page_area: [i32; 4],
    pub content: i32,
    pub fields: &'a mut HashMap<String, Field>,
    pub focused: Option<String>,
    pub blink: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Primary,
    Normal,
    Danger,
    Subtle,
}

pub fn style_color(style: Style, hover: bool) -> u32 {
    match style {
        Style::Primary => if hover { gfx::GREEN_HI } else { gfx::GREEN },
        Style::Danger => if hover { gfx::RED_HI } else { gfx::RED },
        Style::Subtle => if hover { gfx::HOVER } else { 0 },
        Style::Normal => if hover { gfx::HOVER } else { gfx::RAISED },
    }
}

impl<'a> Ctx<'a> {
    pub fn inside(&self, x: i32, y: i32, w: i32, h: i32) -> bool {
        let [mx, my] = self.mouse;
        mx >= x && mx < x + w && my >= y && my < y + h
    }

    pub fn hit(&mut self, x: i32, y: i32, w: i32, h: i32, left: Action) {
        self.hits.push(Hit { rect: [x, y, w, h], left, right: None });
    }

    pub fn hit2(&mut self, x: i32, y: i32, w: i32, h: i32, left: Action, right: Action) {
        self.hits.push(Hit { rect: [x, y, w, h], left, right: Some(right) });
    }

    pub fn button(&mut self, x: i32, y: i32, w: i32, h: i32, label: &str, color: u32, hover_color: u32, text: u32, action: Action) {
        let hover = self.inside(x, y, w, h);
        let c = if hover { hover_color } else { color };
        if c != 0 {
            self.g.panel(x, y, w, h, c);
        }
        let shown = self.g.clip(label, w - 6);
        self.g.centered(&shown, x + w / 2, y + (h - 8) / 2, text);
        self.hit(x, y, w, h, action);
    }

    pub fn icon_button(&mut self, x: i32, y: i32, w: i32, h: i32, icon: &Icon, color: u32, hover_color: u32, icon_color: u32, action: Action) {
        let hover = self.inside(x, y, w, h);
        let c = if hover { hover_color } else { color };
        if c != 0 {
            self.g.panel(x, y, w, h, c);
        }
        let ic = if hover && color == 0 { gfx::WHITE } else { icon_color };
        self.g.icon(icon, x + (w - 9) / 2, y + (h - 9) / 2, ic);
        self.hit(x, y, w, h, action);
    }

    /// The addons' styled button, clipped to the page area.
    pub fn styled(&mut self, x: i32, y: i32, w: i32, h: i32, label: &str, style: Style, action: Option<Action>) {
        let hover = self.page_hover(x, y, w, h);
        let c = style_color(style, hover);
        if c != 0 {
            self.g.panel(x, y, w, h, c);
        }
        let shown = self.g.clip(label, w - 6);
        let tc = if style == Style::Subtle && !hover { gfx::MUTED } else { gfx::WHITE };
        self.g.centered(&shown, x + w / 2, y + (h - 8) / 2, tc);
        if let Some(a) = action {
            self.page_hit(x, y, w, h, a);
        }
    }

    pub fn page_hover(&self, x: i32, y: i32, w: i32, h: i32) -> bool {
        let [px, py, pw, ph] = self.page_area;
        self.inside(x, y, w, h) && self.inside(px, py, pw, ph)
    }

    pub fn page_hit(&mut self, x: i32, y: i32, w: i32, h: i32, action: Action) {
        let [_, py, _, ph] = self.page_area;
        let top = y.max(py);
        let bottom = (y + h).min(py + ph);
        if bottom > top {
            self.hit(x, top, w, bottom - top, action);
        }
    }

    /// The addons' on/off switch.
    pub fn toggle(&mut self, x: i32, y: i32, on: bool, action: Action) {
        let hover = self.page_hover(x, y, 30, 14);
        let c = if on { if hover { gfx::GREEN_HI } else { gfx::GREEN } } else if hover { gfx::FAINT } else { 0xFF4E_5058 };
        self.g.panel(x, y, 30, 14, c);
        self.g.panel(if on { x + 18 } else { x + 2 }, y + 2, 10, 10, gfx::WHITE);
        self.page_hit(x, y, 30, 14, action);
    }

    pub fn section(&mut self, title: &str, x: i32, y: i32, w: i32) -> i32 {
        self.g.text(title, x, y, gfx::FAINT);
        let tw = self.g.width(title);
        self.g.fill(x + tw + 6, y + 4, w - tw - 6, 1, gfx::LINE);
        y + 14
    }

    pub fn avatar(&mut self, x: i32, y: i32, size: i32, uuid: &str, name: &str, status: Option<&str>) {
        self.g.face(uuid, name, x, y, size);
        if let Some(status) = status {
            let d = if size >= 20 { 8 } else { 6 };
            let (dx, dy) = (x + size - d + 2, y + size - d + 2);
            self.g.fill(dx - 1, dy - 1, d + 2, d + 2, gfx::PANEL);
            self.g.fill(dx, dy, d, d, state::status_color(Some(status)));
        }
    }

    /// A single-line text field `id`; returns its value.
    pub fn field(&mut self, id: &str, x: i32, y: i32, w: i32, hint: &str, max: usize) -> String {
        let f = self.fields.entry(id.to_owned()).or_insert_with(|| Field { value: String::new(), max });
        f.max = max;
        let value = f.value.clone();
        let focused = self.focused.as_deref() == Some(id);
        let w = w.max(10);
        if value.is_empty() {
            let hint = self.g.clip(hint, w);
            self.g.text(&hint, x, y, gfx::FAINT);
        } else {
            let mut shown = value.clone();
            while self.g.width(&shown) > w - 6 && !shown.is_empty() {
                shown.remove(0);
            }
            self.g.text(&shown, x, y, gfx::TEXT);
        }
        if focused && self.blink {
            let cx = if value.is_empty() { x } else { x + self.g.width(&value).min(w - 6) + 1 };
            self.g.fill(cx, y - 1, 1, 10, gfx::TEXT);
        }
        self.hit(x - 4, y - 5, w + 8, 18, Action::Focus(id.to_owned()));
        value
    }

    pub fn set_field(&mut self, id: &str, value: &str) {
        self.fields.entry(id.to_owned()).or_default().value = value.to_owned();
    }
}

fn rank_label(rank: &str) -> String {
    match rank {
        "owner" => "Owner".into(),
        "mod" => "Mod".into(),
        "tester" => "Tester".into(),
        r => {
            let mut c = r.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    }
}

fn rank_badge(g: &mut Gfx, x: i32, y: i32, rank: Option<&str>) -> i32 {
    let Some(rank) = rank else { return 0 };
    let label = rank_label(rank);
    let bw = g.width(&label) + 6;
    let color = match rank {
        "owner" => gfx::GOLD,
        "mod" => gfx::BLUE,
        "tester" => 0xFF1A_BC9C,
        _ => gfx::FAINT,
    };
    g.panel(x, y, bw, 10, color);
    g.text(&label, x + 3, y + 1, if rank == "owner" { gfx::PANEL } else { gfx::WHITE });
    bw + 4
}

fn badge(g: &mut Gfx, right: i32, y: i32, count: i64) {
    let s = if count > 99 { "99+".to_owned() } else { count.to_string() };
    let bw = (g.width(&s) + 6).max(12);
    g.panel(right - bw, y, bw, 11, gfx::RED);
    g.centered(&s, right - bw / 2, y + 2, gfx::WHITE);
}

fn group_avatar(g: &mut Gfx, x: i32, y: i32, size: i32, name: &str) {
    let color = gfx::AVATAR_COLORS[crate::util::java_hash(name).wrapping_mul(31).rem_euclid(8) as usize];
    g.panel(x, y, size, size, color);
    g.icon(&gfx::PEOPLE, x + (size - 9) / 2, y + (size - 9) / 2, gfx::WHITE);
}

fn time(ts: i64) -> String {
    use chrono::{Local, TimeZone};
    let Some(t) = Local.timestamp_millis_opt(ts).single() else { return String::new() };
    let today = Local::now().date_naive();
    if t.date_naive() == today {
        tf("Today at {0}", &[&t.format("%-I:%M %p").to_string()])
    } else if Some(t.date_naive()) == today.pred_opt() {
        tf("Yesterday at {0}", &[&t.format("%-I:%M %p").to_string()])
    } else {
        t.format("%b %-d, %-I:%M %p").to_string()
    }
}

fn trust(uuid: &str) -> Trust {
    crate::with_secure(|s| s.trust(uuid))
}

fn trust_note(uuid: &str) -> String {
    match trust(uuid) {
        Trust::Changed => format!("  -  {}", t("key changed, verify!")),
        Trust::None => format!("  -  {}", t("no encryption key yet")),
        Trust::Trusted => String::new(),
    }
}

pub fn opened(c: &mut Core) {
    c.ui.open = true;
    c.ui.close_requested = false;
    if c.ui.page == Page::Chat {
        if let Some(conv) = c.ui.conv.clone() {
            c.open_conversation(Some(&conv));
        }
    }
}

pub fn closed(c: &mut Core) {
    c.ui.open = false;
    c.ui.menu = None;
    c.ui.modal = None;
    c.ui.focused = None;
    c.ui.dragging = None;
    c.st.viewing = None;
    if c.ui.previewing {
        c.ui.previewing = false;
        c.fx.push(Effect::RingStop);
    }
    if crate::voice().mic_test() {
        crate::voice().set_mic_test(false);
    }
}

pub fn open_addon_page(c: &mut Core, id: &str, arg: Option<String>) {
    if !addons::page_exists(c, id) {
        return;
    }
    c.ui.addon_page = Some(id.to_owned());
    go(c, Page::Addon);
    addons::page_opened(c, id, arg);
    if !c.ui.open {
        c.fx.push(Effect::OpenScreen);
    }
}

pub fn confirm_join_server(c: &mut Core, name: Option<String>, address: String) {
    c.ui.pending_join = Some(ServerInvite { name: name.unwrap_or_else(|| address.clone()), address });
    open_modal(c, Modal::JoinServer, None);
    if !c.ui.open {
        c.fx.push(Effect::OpenScreen);
    }
}

fn go(c: &mut Core, p: Page) {
    c.ui.page = p;
    c.ui.page_scroll = 0;
    c.ui.menu = None;
    if p != Page::Chat {
        c.st.viewing = None;
    }
}

fn open_chat(c: &mut Core, conv: &str) {
    if c.ui.conv.as_deref() != Some(conv) {
        c.ui.reply_to = None;
        c.ui.chat_scroll = 0;
    }
    c.ui.conv = Some(conv.to_owned());
    go(c, Page::Chat);
    c.open_conversation(Some(conv));
}

fn open_modal(c: &mut Core, m: Modal, subject: Option<String>) {
    c.ui.modal = Some(m);
    c.ui.modal_subject = subject.clone();
    c.ui.menu = None;
    if m == Modal::RenameGroup {
        let name = subject.as_deref().and_then(|s| c.group(s)).map(|g| g.name.clone()).unwrap_or_default();
        c.ui.fields.entry("rename".into()).or_default().value = name;
        c.ui.focused = Some("rename".into());
    }
    if m == Modal::FriendNote {
        let uuid = subject.unwrap_or_default();
        let nick = c.cfg.nicknames.get(&uuid).cloned().unwrap_or_default();
        let note = c.cfg.notes.get(&uuid).cloned().unwrap_or_default();
        c.ui.fields.entry("nick".into()).or_default().value = nick;
        c.ui.fields.entry("note".into()).or_default().value = note;
        c.ui.focused = Some("nick".into());
    }
}

fn field_value(c: &Core, id: &str) -> String {
    c.ui.fields.get(id).map(|f| f.value.clone()).unwrap_or_default()
}

fn set_field(c: &mut Core, id: &str, v: &str) {
    c.ui.fields.entry(id.into()).or_default().value = v.to_owned();
}

pub fn draw(c: &mut Core, width: f32, height: f32, mouse: [f32; 2], measure: &dyn Fn(&str) -> f32) -> Vec<Draw> {
    let (width, height) = (width as i32, height as i32);
    c.ui.size = [width, height];
    c.ui.mouse = [mouse[0] as i32, mouse[1] as i32];
    if let Some(f) = c.st.focus_request.take() {
        open_chat(c, &f);
    }
    if c.ui.page == Page::Chat && !c.conversation_exists(c.ui.conv.as_deref()) && crate::relay().connected() {
        c.ui.page = Page::Friends;
    }
    if c.ui.page == Page::Call && c.st.call.is_none() {
        c.ui.page = Page::Friends;
    }
    let mut fields = std::mem::take(&mut c.ui.fields);
    let focused = c.ui.focused.clone();
    let mut ctx = Ctx {
        g: Gfx::new(measure),
        hits: Vec::new(),
        mouse: c.ui.mouse,
        tooltip: None,
        page_area: c.ui.page_area,
        content: 0,
        fields: &mut fields,
        focused,
        blink: (now_ms() / 500) % 2 == 0,
    };
    ctx.g.fill(0, 0, width, height, 0xD010_1113);
    let w = (width - 16).min(720);
    let h = height - 16;
    let x0 = (width - w) / 2;
    let y0 = 8;
    ctx.g.panel(x0, y0, w, h, gfx::MAIN);
    let side_w = 200.min(150.max(w / 3));
    draw_sidebar(c, &mut ctx, x0, y0, side_w, h);
    let (main_x, main_w) = (x0 + side_w, w - side_w);
    if !crate::relay().connected() && c.ui.page != Page::Settings {
        draw_disconnected(c, &mut ctx, main_x, y0, main_w, h);
    } else {
        match c.ui.page {
            Page::Friends => draw_friends(c, &mut ctx, main_x, y0, main_w, h),
            Page::Chat => draw_chat(c, &mut ctx, main_x, y0, main_w, h),
            Page::NewGroup => draw_new_group(c, &mut ctx, main_x, y0, main_w, h),
            Page::Call => draw_call(c, &mut ctx, main_x, y0, main_w, h),
            Page::Settings => draw_settings(c, &mut ctx, main_x, y0, main_w, h),
            Page::Addon => draw_addon(c, &mut ctx, main_x, y0, main_w, h),
        }
    }
    draw_notice(c, &mut ctx, main_x, main_w, y0);
    if c.ui.modal.is_some() {
        ctx.hits.clear();
        draw_modal(c, &mut ctx, width, height);
    }
    draw_overlay(c, &mut ctx, width, height);
    c.ui.hits = std::mem::take(&mut ctx.hits);
    c.ui.page_area = ctx.page_area;
    if ctx.focused.as_ref().is_some_and(|f| !ctx.hits_had_field(f, &c.ui.hits)) {
        ctx.focused = None;
    }
    c.ui.focused = ctx.focused.clone();
    let out = ctx.g.finish();
    c.ui.fields = fields;
    out
}

impl Ctx<'_> {
    fn hits_had_field(&self, id: &str, hits: &[Hit]) -> bool {
        hits.iter().any(|h| matches!(&h.left, Action::Focus(f) if f == id))
    }
}

fn draw_overlay(c: &mut Core, ctx: &mut Ctx, width: i32, height: i32) {
    c.ui.overlay_hits.clear();
    if let Some(tip) = ctx.tooltip.clone().filter(|_| c.ui.menu.is_none() && c.ui.modal.is_none()) {
        let tip = ctx.g.clip(&tip, 300.min(width - 16));
        let tw = ctx.g.width(&tip) + 10;
        let tx = 4.max((ctx.mouse[0] - tw / 2).min(width - tw - 4));
        let ty = 4.max(ctx.mouse[1] - 22);
        ctx.g.panel(tx, ty, tw, 16, gfx::SHADOW);
        ctx.g.text(&tip, tx + 5, ty + 4, gfx::WHITE);
    }
    let Some((items, menu_x, menu_y)) = c.ui.menu.clone() else { return };
    let mut mw = 40;
    for it in &items {
        mw = mw.max(ctx.g.width(&it.label) + 20);
    }
    let mh = items.len() as i32 * 16 + 6;
    let mx = menu_x.min(width - mw - 4);
    let my = menu_y.min(height - mh - 4);
    ctx.g.panel(mx - 1, my - 1, mw + 2, mh + 2, gfx::SHADOW);
    ctx.g.panel(mx, my, mw, mh, gfx::MENU);
    let mut y = my + 3;
    for it in items {
        let hover = ctx.inside(mx + 3, y, mw - 6, 16);
        if it.action.is_some() && hover {
            ctx.g.panel(mx + 3, y, mw - 6, 16, if it.color == gfx::RED { gfx::RED } else { gfx::BLUE });
        }
        let color = if it.action.is_none() { gfx::FAINT } else if hover { gfx::WHITE } else { it.color };
        ctx.g.text(&it.label, mx + 9, y + 4, color);
        if let Some(a) = it.action {
            c.ui.overlay_hits.push(Hit { rect: [mx + 3, y, mw - 6, 16], left: a, right: None });
        }
        y += 16;
    }
}

fn nav_item(c: &Core, ctx: &mut Ctx, x: i32, y: i32, sw: i32, icon: &Icon, label: &str, selected: bool, badge_n: i64, action: Action) -> i32 {
    let _ = c;
    let hover = ctx.inside(x + 6, y, sw - 12, 24);
    if selected || hover {
        ctx.g.panel(x + 6, y, sw - 12, 24, if selected { gfx::SELECTED } else { gfx::HOVER });
    }
    ctx.g.icon(icon, x + 13, y + 8, if selected { gfx::WHITE } else { gfx::MUTED });
    ctx.g.text(label, x + 30, y + 8, if !selected && !hover { gfx::MUTED } else { gfx::WHITE });
    if badge_n > 0 {
        badge(&mut ctx.g, x + sw - 14, y + 7, badge_n);
    }
    ctx.hit(x + 6, y, sw - 12, 24, action);
    y + 26
}

fn section_header(ctx: &mut Ctx, x: i32, y: i32, sw: i32, label: &str, plus: Option<Action>) -> i32 {
    ctx.g.text(label, x + 12, y + 4, gfx::FAINT);
    if let Some(a) = plus {
        let hover = ctx.inside(x + sw - 22, y, 14, 14);
        ctx.g.icon(&gfx::PLUS, x + sw - 20, y + 3, if hover { gfx::WHITE } else { gfx::MUTED });
        ctx.hit(x + sw - 22, y, 14, 14, a);
    }
    y + 16
}

#[allow(clippy::too_many_arguments)]
fn conv_row(c: &Core, ctx: &mut Ctx, x: i32, y: i32, sw: i32, conv: &str, name: &str, status: Option<&str>, sub: Option<&str>, id: &str) -> i32 {
    let selected = c.ui.page == Page::Chat && c.ui.conv.as_deref() == Some(conv);
    let hover = ctx.inside(x + 6, y, sw - 12, 26);
    if selected || hover {
        ctx.g.panel(x + 6, y, sw - 12, 26, if selected { gfx::SELECTED } else { gfx::HOVER });
    }
    let group = conv.starts_with("g:");
    if group {
        group_avatar(&mut ctx.g, x + 11, y + 4, 18, name);
    } else {
        ctx.avatar(x + 11, y + 4, 18, id, name, status);
    }
    let unread = c.st.unread.get(conv).copied().unwrap_or(0);
    let muted_group = c.group_muted(conv);
    let dim = muted_group || (!group && !matches!(status, Some("online" | "idle" | "dnd")));
    let name_color = if selected || unread > 0 { gfx::WHITE } else if hover { gfx::TEXT } else if dim { gfx::FAINT } else { gfx::MUTED };
    let text_w = sw - 46 - if unread > 0 { 18 } else { 0 };
    if let Some(sub) = sub {
        let n = ctx.g.clip(name, text_w);
        ctx.g.text(&n, x + 35, y + 4, name_color);
        let s = ctx.g.clip(sub, text_w);
        ctx.g.text(&s, x + 35, y + 15, gfx::FAINT);
    } else {
        let rank = c.rank(Some(id));
        let rw = rank.as_deref().map_or(0, |r| ctx.g.width(&rank_label(r)) + 10);
        let shown = ctx.g.clip(name, text_w - rw);
        ctx.g.text(&shown, x + 35, y + 9, name_color);
        let sw2 = ctx.g.width(&shown);
        rank_badge(&mut ctx.g, x + 39 + sw2, y + 8, rank.as_deref());
    }
    if unread > 0 && muted_group {
        let s = if unread > 99 { "99+".to_owned() } else { unread.to_string() };
        let bw = (ctx.g.width(&s) + 6).max(12);
        ctx.g.panel(x + sw - 16 - bw, y + 8, bw, 11, gfx::FAINT);
        ctx.g.centered(&s, x + sw - 16 - bw / 2, y + 10, gfx::WHITE);
    } else if unread > 0 {
        badge(&mut ctx.g, x + sw - 16, y + 8, unread);
    } else if muted_group {
        ctx.g.icon(&BELL_OFF, x + sw - 24, y + 9, gfx::FAINT);
    }
    ctx.hit2(x + 6, y, sw - 12, 26, Action::OpenChat(conv.to_owned()), Action::ConvMenu(conv.to_owned()));
    y + 28
}

fn draw_sidebar(c: &mut Core, ctx: &mut Ctx, x: i32, y: i32, sw: i32, sh: i32) {
    ctx.g.fill(x, y + 1, sw, sh - 2, gfx::PANEL);
    ctx.g.fill(x + 1, y, sw - 1, 1, gfx::PANEL);
    ctx.g.logo(x + 8, y + 7);
    ctx.g.text("VCraft", x + 30, y + 11, gfx::WHITE);
    let st = crate::relay().status();
    let dot = match st {
        Status::Connected => gfx::GREEN,
        Status::Disconnected => gfx::RED,
        _ => gfx::GOLD,
    };
    ctx.g.fill(x + sw - 14, y + 13, 5, 5, dot);
    ctx.g.fill(x, y + 30, sw, 1, gfx::LINE);
    let bottom = y + sh - 40;
    let call_h = if c.st.call.is_some() { 46 } else { 0 };
    let live = c.live_invites();
    let request = c.social.requests.last().cloned();
    let invite_h = if live.is_empty() && request.is_none() { 0 } else { 40 };
    let list_top = y + 34;
    let list_bottom = bottom - call_h - invite_h;
    c.ui.sb = [x, list_top, sw, list_bottom - list_top];
    ctx.g.scissor(x, list_top, sw, list_bottom - list_top);
    let mut ly = list_top - c.ui.sidebar_scroll;
    let pending = c.st.incoming.len() as i64;
    ly = nav_item(c, ctx, x, ly, sw, &gfx::PEOPLE, t("Friends"), c.ui.page == Page::Friends, pending, Action::Go(Page::Friends));
    let lobby_sel = c.ui.page == Page::Chat && c.ui.conv.as_deref() == Some(LOBBY);
    let lobby_unread = c.st.unread.get(LOBBY).copied().unwrap_or(0);
    ly = nav_item(c, ctx, x, ly, sw, &gfx::GLOBE, t("VCraft Community"), lobby_sel, lobby_unread, Action::OpenChat(LOBBY.into()));
    for p in addons::pages(c) {
        if p.in_sidebar {
            let sel = c.ui.page == Page::Addon && c.ui.addon_page.as_deref() == Some(p.id);
            let title = ctx.g.clip(&p.title, sw - 50);
            ly = nav_item(c, ctx, x, ly, sw, &p.icon, &title, sel, p.badge, Action::OpenAddonPage(p.id.into(), None));
        }
    }
    ly += 6;
    ly = section_header(ctx, x, ly, sw, t("DIRECT MESSAGES"), None);
    let friends = c.st.friends.clone();
    for f in &friends {
        let name = c.cfg.display_name(&f.uuid, &f.name);
        ly = conv_row(c, ctx, x, ly, sw, &f.uuid, &name, Some(&f.status), None, &f.uuid);
    }
    if friends.is_empty() && crate::relay().connected() {
        ctx.g.text(t("No friends yet"), x + 12, ly + 4, gfx::FAINT);
        ly += 16;
    }
    ly += 6;
    ly = section_header(ctx, x, ly, sw, t("GROUPS"), Some(Action::NewGroup));
    let groups = c.st.groups.clone();
    for gr in &groups {
        let on = gr.members.iter().filter(|p| p.online()).count();
        let sub = if c.group_muted(&gr.conv()) {
            t("Muted").to_owned()
        } else {
            tf("{0} of {1} online", &[&on.to_string(), &gr.members.len().to_string()])
        };
        ly = conv_row(c, ctx, x, ly, sw, &gr.conv(), &gr.name, None, Some(&sub), &gr.id);
    }
    if groups.is_empty() && crate::relay().connected() {
        ctx.g.text(t("No groups yet"), x + 12, ly + 4, gfx::FAINT);
        ly += 16;
    }
    ctx.g.end_scissor();
    c.ui.sidebar_max = 0.max(ly + c.ui.sidebar_scroll - list_bottom + 6);
    c.ui.sidebar_scroll = c.ui.sidebar_scroll.min(c.ui.sidebar_max);
    let iy = list_bottom;
    if let Some(inv) = live.last() {
        ctx.g.fill(x, iy, sw, invite_h, gfx::MENU);
        ctx.g.fill(x, iy, 2, invite_h, gfx::GREEN);
        let n = ctx.g.clip(&c.cfg.display_name(&inv.from, &inv.from_name), sw - 70);
        ctx.g.text(&n, x + 8, iy + 6, gfx::WHITE);
        ctx.g.text(t("is calling..."), x + 8, iy + 18, gfx::MUTED);
        ctx.icon_button(x + sw - 54, iy + 9, 22, 22, &gfx::PHONE, gfx::GREEN, gfx::GREEN_HI, gfx::WHITE, Action::AcceptInvite(inv.call_id.clone()));
        ctx.icon_button(x + sw - 28, iy + 9, 22, 22, &gfx::HANGUP, gfx::RED, gfx::RED_HI, gfx::WHITE, Action::DeclineInvite(inv.call_id.clone()));
    } else if let Some(r) = request {
        ctx.g.fill(x, iy, sw, invite_h, gfx::MENU);
        ctx.g.fill(x, iy, 2, invite_h, gfx::GOLD);
        let n = ctx.g.clip(&r.name, sw - 70);
        ctx.g.text(&n, x + 8, iy + 6, gfx::WHITE);
        let what = match &r.kind {
            RequestKind::Call => t("wants to join your call").to_owned(),
            RequestKind::Group(g) => tf("wants to join {0}", &[&c.group(&format!("g:{g}")).map(|g| g.name.clone()).unwrap_or_default()]),
        };
        let what = ctx.g.clip(&what, sw - 70);
        ctx.g.text(&what, x + 8, iy + 18, gfx::MUTED);
        ctx.icon_button(x + sw - 54, iy + 9, 22, 22, &gfx::CHECK, gfx::GREEN, gfx::GREEN_HI, gfx::WHITE, Action::AcceptRequest(r.from.clone(), r.kind.clone()));
        ctx.icon_button(x + sw - 28, iy + 9, 22, 22, &gfx::CLOSE, gfx::RED, gfx::RED_HI, gfx::WHITE, Action::DismissRequest(r.from.clone(), r.kind.clone()));
    }
    if let Some(call) = c.st.call.clone() {
        let cy = bottom - call_h;
        ctx.g.fill(x, cy, sw, 1, gfx::LINE);
        ctx.g.fill(x, cy + 1, sw, call_h - 1, gfx::MENU);
        let hover_call = ctx.inside(x, cy + 1, sw - 60, call_h - 1);
        ctx.g.text(t("Voice Connected"), x + 8, cy + 9, if hover_call { gfx::GREEN_HI } else { gfx::GREEN });
        let n = call.members.len();
        let with = if n <= 1 {
            t("Waiting for others...").to_owned()
        } else if n == 2 {
            t("1 other in call").to_owned()
        } else {
            tf("{0} others in call", &[&(n - 1).to_string()])
        };
        let with = ctx.g.clip(&with, sw - 66);
        ctx.g.text(&with, x + 8, cy + 21, gfx::MUTED);
        ctx.hit(x, cy + 1, sw - 60, call_h - 1, Action::Go(Page::Call));
        ctx.icon_button(x + sw - 54, cy + 12, 22, 22, &gfx::REFRESH, gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::ReconnectVoice);
        ctx.icon_button(x + sw - 28, cy + 12, 22, 22, &gfx::HANGUP, gfx::RED, gfx::RED_HI, gfx::WHITE, Action::Relay("call_leave", None, None));
    }
    ctx.g.fill(x, bottom, sw, 40, gfx::MENU);
    let name = c.my_name();
    let hover_me = ctx.inside(x + 4, bottom + 4, sw - 72, 32);
    if hover_me {
        ctx.g.panel(x + 4, bottom + 4, sw - 72, 32, gfx::HOVER);
    }
    let my_id = c.st.my_uuid.clone().or_else(|| c.account.as_ref().and_then(|a| a.uuid.clone())).unwrap_or_default();
    let my_status = if c.st.my_uuid.is_none() { "offline".to_owned() } else { c.st.my_status.clone() };
    ctx.avatar(x + 9, bottom + 10, 20, &my_id, &name, Some(&my_status));
    let shown = ctx.g.clip(&name, sw - 110);
    ctx.g.text(&shown, x + 35, bottom + 9, gfx::WHITE);
    let shown_w = ctx.g.width(&shown);
    if 35 + shown_w + 40 < sw - 66 {
        rank_badge(&mut ctx.g, x + 39 + shown_w, bottom + 8, c.st.my_rank.as_deref());
    }
    let sub = if crate::relay().connected() { t(state::status_label(Some(&c.st.my_status))).to_owned() } else { t("Not connected").to_owned() };
    let sub = ctx.g.clip(&sub, sw - 110);
    ctx.g.text(&sub, x + 35, bottom + 21, gfx::MUTED);
    ctx.hit(x + 4, bottom + 4, sw - 72, 32, Action::StatusMenu(x + 6, bottom - 74));
    let bx = x + sw - 66;
    toggle_icon(ctx, bx, bottom + 9, &gfx::MIC, c.st.muted || c.st.deafened, Action::ToggleMute);
    toggle_icon(ctx, bx + 21, bottom + 9, &gfx::HEADPHONES, c.st.deafened, Action::ToggleDeafen);
    let gear = if c.ui.page == Page::Settings { gfx::WHITE } else { gfx::MUTED };
    ctx.icon_button(bx + 42, bottom + 9, 20, 22, &gfx::GEAR, 0, gfx::HOVER, gear, Action::Go(Page::Settings));
}

fn toggle_icon(ctx: &mut Ctx, x: i32, y: i32, icon: &Icon, off: bool, action: Action) {
    let hover = ctx.inside(x, y, 20, 22);
    if hover {
        ctx.g.panel(x, y, 20, 22, gfx::HOVER);
    }
    if off {
        ctx.g.icon_struck(icon, x + 6, y + 7, gfx::RED);
    } else {
        ctx.g.icon(icon, x + 6, y + 7, if hover { gfx::WHITE } else { gfx::MUTED });
    }
    ctx.hit(x, y, 20, 22, action);
}

fn big_toggle(ctx: &mut Ctx, x: i32, y: i32, icon: &Icon, off: bool, action: Action) {
    let hover = ctx.inside(x, y, 40, 32);
    let c = if off { if hover { 0xFFF5_F5F5 } else { gfx::TEXT } } else if hover { gfx::HOVER } else { gfx::RAISED };
    ctx.g.panel(x, y, 40, 32, c);
    if off {
        ctx.g.icon_struck(icon, x + 16, y + 12, gfx::BG);
    } else {
        ctx.g.icon(icon, x + 16, y + 12, gfx::WHITE);
    }
    ctx.hit(x, y, 40, 32, action);
}

fn header(ctx: &mut Ctx, x: i32, y: i32, w: i32) {
    ctx.g.fill(x, y + 33, w, 1, gfx::LINE);
}

fn set_page_area(c: &mut Core, ctx: &mut Ctx, x: i32, y: i32, w: i32, h: i32) {
    ctx.page_area = [x, y, w, h.max(10)];
    c.ui.page_scroll = 0.max(c.ui.page_scroll.min(c.ui.page_max));
}

fn draw_friends(c: &mut Core, ctx: &mut Ctx, x: i32, y: i32, pw: i32, ph: i32) {
    header(ctx, x, y, pw);
    ctx.g.icon(&gfx::PEOPLE, x + 12, y + 12, gfx::MUTED);
    ctx.g.text(t("Friends"), x + 28, y + 13, gfx::WHITE);
    let mut tx = x + 28 + ctx.g.width(t("Friends")) + 14;
    ctx.g.fill(tx - 7, y + 10, 1, 14, gfx::LINE);
    for (i, label) in [t("Online"), t("All"), t("Pending"), t("Blocked")].iter().enumerate() {
        let tw = ctx.g.width(label) + 12;
        let sel = c.ui.friends_tab == i;
        let hover = ctx.inside(tx, y + 8, tw, 18);
        if sel || hover {
            ctx.g.panel(tx, y + 8, tw, 18, if sel { gfx::SELECTED } else { gfx::HOVER });
        }
        ctx.g.text(label, tx + 6, y + 13, if sel { gfx::WHITE } else { gfx::MUTED });
        ctx.hit(tx, y + 8, tw, 18, Action::FriendsTab(i));
        let pend = i == 2 && !c.st.incoming.is_empty();
        if pend {
            badge(&mut ctx.g, tx + tw + 2, y + 12, c.st.incoming.len() as i64);
        }
        tx += tw + if pend { 18 } else { 4 };
    }
    draw_kofi(ctx, tx, x + pw - 10, y + 8);
    let cy = y + 44;
    ctx.g.text(t("ADD FRIEND"), x + 16, cy, gfx::WHITE);
    ctx.g.text(t("Send a request with their Minecraft username."), x + 16, cy + 12, gfx::MUTED);
    let field_y = cy + 26;
    ctx.g.panel(x + 16, field_y, pw - 32, 26, gfx::BG);
    let value = ctx.field("addFriend", x + 24, field_y + 9, pw - 140, t("Enter a Minecraft username"), 16);
    let can = !value.trim().is_empty();
    let (bc, bh, bt) = if can { (gfx::GREEN, gfx::GREEN_HI, gfx::WHITE) } else { (gfx::DISABLED, gfx::DISABLED, gfx::MUTED) };
    ctx.button(x + pw - 112, field_y + 4, 92, 18, t("Send Request"), bc, bh, bt, Action::SendFriendRequest);
    // Join a group with a code someone shared.
    let code_y = field_y + 30;
    ctx.g.panel(x + 16, code_y, pw - 32, 22, gfx::BG);
    let code = ctx.field("groupCode", x + 24, code_y + 7, pw - 140, t("Paste a group invite code (vc1...)"), 400);
    let can = !code.trim().is_empty();
    let (bc, bh, bt) = if can { (gfx::RAISED, gfx::HOVER, gfx::WHITE) } else { (gfx::DISABLED, gfx::DISABLED, gfx::MUTED) };
    ctx.button(x + pw - 112, code_y + 2, 92, 18, t("Join Group"), bc, bh, bt, Action::JoinWithCode);
    let mut list_y = code_y + 34;
    let tab = c.ui.friends_tab;
    let title = [t("ONLINE"), t("ALL FRIENDS"), t("PENDING"), t("BLOCKED")][tab.min(3)];
    let people: Vec<Profile> = match tab {
        0 => c.st.friends.iter().filter(|p| p.online()).cloned().collect(),
        1 => c.st.friends.clone(),
        2 => c.st.incoming.iter().chain(c.st.outgoing.iter()).cloned().collect(),
        _ => c.st.blocked.clone(),
    };
    ctx.g.text(&format!("{title} - {}", people.len()), x + 16, list_y, gfx::FAINT);
    list_y += 14;
    set_page_area(c, ctx, x, list_y, pw, y + ph - list_y - 4);
    let [px, py, pw2, ph2] = ctx.page_area;
    ctx.g.scissor(px, py, pw2, ph2);
    let mut ry = list_y - c.ui.page_scroll;
    for p in &people {
        ry = friend_row(c, ctx, x + 10, ry, pw - 20, p);
    }
    if people.is_empty() {
        let empty = match tab {
            0 => t("None of your friends are online right now."),
            1 => t("Add a friend above to get started."),
            2 => t("No pending friend requests."),
            _ => t("You haven't blocked anyone."),
        };
        ctx.g.centered(empty, x + pw / 2, list_y + 30, gfx::FAINT);
    }
    ctx.g.end_scissor();
    c.ui.page_max = 0.max(ry + c.ui.page_scroll - (py + ph2));
}

fn listening_line(ctx: &mut Ctx, song: &str, x: i32, y: i32, max_w: i32) {
    ctx.g.icon(&gfx::NOTE, x, y, gfx::GREEN);
    let s = ctx.g.clip(song, 20.max(max_w - 12));
    ctx.g.text(&s, x + 12, y, gfx::LIGHT_GREEN);
}

fn draw_kofi(ctx: &mut Ctx, min_x: i32, right: i32, y: i32) {
    let label = t("Support me");
    let full = ctx.g.width(label) + 26;
    let compact = right - full < min_x + 6;
    let bw = if compact { 20 } else { full };
    let bx = right - bw;
    if bx < min_x {
        return;
    }
    let hover = ctx.inside(bx, y, bw, 18);
    ctx.g.panel(bx, y, bw, 18, if hover { 0xFFFF_7A77 } else { 0xFFFF_5E5B });
    ctx.g.icon(&gfx::HEART, bx + 6, y + 5, gfx::WHITE);
    if !compact {
        ctx.g.text(label, bx + 19, y + 5, gfx::WHITE);
    }
    ctx.hit(bx, y, bw, 18, Action::Kofi);
}

fn friend_row(c: &Core, ctx: &mut Ctx, x: i32, y: i32, rw: i32, p: &Profile) -> i32 {
    ctx.g.fill(x + 6, y, rw - 12, 1, gfx::LINE);
    if ctx.inside(x, y + 1, rw, 40) {
        ctx.g.panel(x, y + 2, rw, 38, gfx::HOVER);
    }
    let incoming = c.st.incoming.contains(p);
    let outgoing = c.st.outgoing.contains(p);
    let blocked = c.ui.friends_tab == 3;
    let plain = !incoming && !outgoing && !blocked;
    ctx.avatar(x + 8, y + 10, 22, &p.uuid, &p.name, plain.then_some(p.status.as_str()));
    let shown = if plain { c.cfg.display_name(&p.uuid, &p.name) } else { p.name.clone() };
    ctx.g.text(&shown, x + 38, y + 10, gfx::WHITE);
    let mut nw = ctx.g.width(&shown);
    if shown != p.name {
        let real = format!("({})", p.name);
        ctx.g.text(&real, x + 42 + nw, y + 10, gfx::FAINT);
        nw += ctx.g.width(&real) + 4;
    }
    rank_badge(&mut ctx.g, x + 42 + nw, y + 9, c.rank(Some(&p.uuid)).as_deref());
    let bx = x + rw - 32;
    let joinable = plain && c.friend_in_call(&p.uuid) && c.st.call.is_none();
    let sub_w = bx - if joinable { 92 } else { 64 } - x - 38;
    let tr = trust(&p.uuid);
    let song = if plain { c.st.listening.get(&p.uuid).cloned() } else { None };
    match song.filter(|_| tr != Trust::Changed) {
        Some(song) => listening_line(ctx, &song, x + 38, y + 22, sub_w),
        None => {
            let sub = if incoming {
                t("Incoming friend request").to_owned()
            } else if outgoing {
                t("Outgoing friend request").to_owned()
            } else if blocked {
                t("Blocked").to_owned()
            } else if tr != Trust::Trusted {
                format!("{}{}", t(state::status_label(Some(&p.status))), trust_note(&p.uuid))
            } else {
                match c.friend_detail(&p.uuid, p.online()) {
                    Some(d) if p.online() => format!("{}  -  {d}", t(state::status_label(Some(&p.status)))),
                    Some(d) => d,
                    None => t(state::status_label(Some(&p.status))).to_owned(),
                }
            };
            let color = if tr == Trust::Changed && !incoming && !outgoing { gfx::RED } else { gfx::MUTED };
            let sub = ctx.g.clip(&sub, sub_w.max(20));
            ctx.g.text(&sub, x + 38, y + 22, color);
        }
    }
    if plain && ctx.inside(x, y + 1, rw - 90, 40) {
        if let Some(note) = c.cfg.notes.get(&p.uuid) {
            ctx.tooltip = Some(note.clone());
        }
    }
    let uuid = p.uuid.clone();
    if incoming {
        ctx.icon_button(bx, y + 9, 24, 24, &gfx::CLOSE, gfx::RAISED, gfx::RED, gfx::MUTED, Action::Relay("friend_remove", Some("uuid"), Some(uuid.clone())));
        ctx.icon_button(bx - 28, y + 9, 24, 24, &gfx::CHECK, gfx::RAISED, gfx::GREEN, gfx::MUTED, Action::Relay("friend_accept", Some("uuid"), Some(uuid)));
    } else if outgoing {
        ctx.icon_button(bx, y + 9, 24, 24, &gfx::CLOSE, gfx::RAISED, gfx::RED, gfx::MUTED, Action::Relay("friend_remove", Some("uuid"), Some(uuid)));
    } else if blocked {
        ctx.button(x + rw - 74, y + 12, 66, 18, t("Unblock"), gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::Relay("unblock", Some("uuid"), Some(uuid)));
    } else {
        ctx.icon_button(bx, y + 9, 24, 24, &gfx::DOTS, gfx::RAISED, gfx::HOVER, gfx::MUTED, Action::ConvMenu(uuid.clone()));
        ctx.icon_button(bx - 28, y + 9, 24, 24, &gfx::PHONE, gfx::RAISED, gfx::HOVER, gfx::MUTED, Action::Call(uuid.clone()));
        ctx.icon_button(bx - 56, y + 9, 24, 24, &gfx::REPLY, gfx::RAISED, gfx::HOVER, gfx::MUTED, Action::OpenChat(uuid.clone()));
        if joinable {
            ctx.icon_button(bx - 84, y + 9, 24, 24, &gfx::PLUS, gfx::GREEN, gfx::GREEN_HI, gfx::WHITE, Action::AskToJoin(uuid.clone()));
            if ctx.inside(bx - 84, y + 9, 24, 24) {
                ctx.tooltip = Some(t("Ask to join their call").to_owned());
            }
        }
        ctx.hit2(x, y + 1, rw - if joinable { 118 } else { 90 }, 40, Action::OpenChat(uuid.clone()), Action::ConvMenu(uuid));
    }
    y + 42
}

fn draw_chat(c: &mut Core, ctx: &mut Ctx, x: i32, y: i32, pw: i32, ph: i32) {
    let conv = c.ui.conv.clone().unwrap_or_default();
    let group = c.group(&conv).cloned();
    let friend = c.friend(&conv).cloned();
    let members = group.is_some() && !c.ui.hide_members && pw > 360;
    let member_w = if members { 130 } else { 0 };
    let cw = pw - member_w;
    header(ctx, x, y, cw);
    let ey = y + 34;
    if conv == LOBBY {
        ctx.g.panel(x + 10, y + 7, 20, 20, gfx::GREEN);
        ctx.g.icon(&gfx::GLOBE, x + 15, y + 12, gfx::WHITE);
        ctx.g.text("VCraft Community", x + 36, y + 9, gfx::WHITE);
        ctx.g.text(&format!("{} online  -  official public chat", c.st.lobby_online), x + 36, y + 20, gfx::MUTED);
        ctx.g.fill(x, ey, cw, 14, 0x30F0_B232);
        let note = if c.st.lobby_muted {
            "You're muted here, but you can still read along."
        } else {
            "Public: everyone can read this, and it isn't end-to-end encrypted. Be kind!"
        };
        let note = ctx.g.clip(note, cw - 20);
        ctx.g.text(&note, x + 10, ey + 3, 0xFFF5_D88A);
        draw_chat_body(c, ctx, x, y, cw, ph, ey, &conv);
        return;
    }
    if let Some(g) = &group {
        group_avatar(&mut ctx.g, x + 10, y + 7, 20, &g.name);
        let n = ctx.g.clip(&g.name, cw - 190);
        ctx.g.text(&n, x + 36, y + 9, gfx::WHITE);
        let count = g.members.len();
        ctx.g.text(&format!("{count} {}", if count == 1 { "member" } else { "members" }), x + 36, y + 20, gfx::MUTED);
    } else if let Some(f) = &friend {
        ctx.avatar(x + 10, y + 7, 20, &f.uuid, &f.name, Some(&f.status));
        let shown = c.cfg.display_name(&f.uuid, &f.name);
        ctx.g.text(&shown, x + 36, y + 9, gfx::WHITE);
        let nw = ctx.g.width(&shown);
        if ctx.inside(x + 36, y + 7, nw, 12) {
            ctx.tooltip = Some(match c.cfg.notes.get(&f.uuid) {
                Some(n) => format!("{}: {n}", f.name),
                None => f.name.clone(),
            });
        }
        rank_badge(&mut ctx.g, x + 40 + nw, y + 8, c.rank(Some(&f.uuid)).as_deref());
        match c.st.listening.get(&f.uuid).cloned() {
            Some(song) => listening_line(ctx, &song, x + 36, y + 20, cw - 220),
            None => {
                let sub = match c.friend_detail(&f.uuid, f.online()) {
                    Some(d) if f.online() => format!("{}  -  {d}", t(state::status_label(Some(&f.status)))),
                    Some(d) => d,
                    None => t(state::status_label(Some(&f.status))).to_owned(),
                };
                let sub = ctx.g.clip(&sub, cw - 220);
                ctx.g.text(&sub, x + 36, y + 20, gfx::MUTED);
            }
        }
    }
    let mut hx = x + cw - 30;
    ctx.icon_button(hx, y + 7, 22, 20, &gfx::DOTS, 0, gfx::HOVER, gfx::MUTED, Action::ConvMenu(conv.clone()));
    let pin_count = c.pins(&conv).len();
    hx -= 24;
    let pin_col = if c.ui.show_pins { gfx::WHITE } else if pin_count > 0 { gfx::TEXT } else { gfx::FAINT };
    ctx.icon_button(hx, y + 7, 22, 20, &PIN, 0, gfx::HOVER, pin_col, Action::TogglePins);
    if ctx.inside(hx, y + 7, 22, 20) {
        ctx.tooltip = Some(tf("Your pinned messages ({0})", &[&pin_count.to_string()]));
    }
    if friend.as_ref().is_some_and(|f| c.friend_in_call(&f.uuid)) && c.st.call.is_none() {
        hx -= 24;
        ctx.icon_button(hx, y + 7, 22, 20, &gfx::PLUS, gfx::GREEN, gfx::GREEN_HI, gfx::WHITE, Action::AskToJoin(conv.clone()));
        if ctx.inside(hx, y + 7, 22, 20) {
            ctx.tooltip = Some(t("Ask to join their call").to_owned());
        }
    }
    if group.is_some() {
        hx -= 24;
        let col = if !c.ui.hide_members { gfx::WHITE } else { gfx::MUTED };
        ctx.icon_button(hx, y + 7, 22, 20, &gfx::PEOPLE, 0, gfx::HOVER, col, Action::ToggleMembers);
    }
    for b in addons::chat_buttons(c, &conv) {
        hx -= 24;
        ctx.icon_button(hx, y + 7, 22, 20, &b.icon, 0, gfx::HOVER, gfx::MUTED, b.action);
        if ctx.inside(hx, y + 7, 22, 20) {
            ctx.tooltip = Some(b.tooltip);
        }
    }
    hx -= 24;
    ctx.icon_button(hx, y + 7, 22, 20, &gfx::SERVER, 0, gfx::HOVER, gfx::MUTED, Action::InviteToServer(conv.clone()));
    hx -= 24;
    let call = if group.is_some() { Action::CallGroup(conv.clone()) } else { Action::Call(conv.clone()) };
    ctx.icon_button(hx, y + 7, 22, 20, &gfx::PHONE, 0, gfx::HOVER, gfx::MUTED, call);
    let tr = if group.is_none() { trust(&conv) } else { Trust::Trusted };
    let changed: Vec<String> = group.as_ref().map_or(Vec::new(), |g| g.members.iter().filter(|p| trust(&p.uuid) == Trust::Changed).map(|p| p.name.clone()).collect());
    let warn = tr != Trust::Trusted || !changed.is_empty();
    ctx.g.fill(x, ey, cw, 14, if warn { 0x40ED_4245 } else { 0x203B_A55C });
    ctx.g.icon(&gfx::LOCK, x + 10, ey + 3, if warn { gfx::RED } else { gfx::GREEN });
    let enc = if group.is_some() {
        if changed.is_empty() { "End-to-end encrypted".to_owned() } else { format!("Key changed for {} - they won't get messages", changed.join(", ")) }
    } else {
        match tr {
            Trust::Changed => "Their encryption key changed. Click to verify".into(),
            Trust::None => "No encryption key yet - they need to sign in once".into(),
            Trust::Trusted => "End-to-end encrypted. Click to verify safety number".into(),
        }
    };
    let enc = ctx.g.clip(&enc, cw - 30);
    ctx.g.text(&enc, x + 24, ey + 3, if warn { gfx::PINK_RED } else { 0xFFA6_E3B8 });
    if group.is_none() {
        ctx.hit(x, ey, cw, 14, Action::OpenModal(Modal::Safety, Some(conv.clone())));
    }
    draw_chat_body(c, ctx, x, y, cw, ph, ey, &conv);
    if let (true, Some(g)) = (members, &group) {
        draw_members(c, ctx, x + cw, y, member_w, ph, g);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_chat_body(c: &mut Core, ctx: &mut Ctx, x: i32, y: i32, cw: i32, ph: i32, ey: i32, conv: &str) {
    let lobby = conv == LOBBY;
    let composer_h = 26;
    let cy = y + ph - composer_h - 10;
    if let Some(r) = c.ui.reply_to.clone() {
        ctx.g.panel(x + 12, cy - 20, cw - 24, 22, gfx::PANEL);
        let t = ctx.g.clip(&format!("Replying to {}:  {}", r.name, r.text), cw - 60);
        ctx.g.text(&t, x + 20, cy - 14, gfx::MUTED);
        ctx.icon_button(x + cw - 30, cy - 18, 14, 14, &gfx::CLOSE, 0, gfx::HOVER, gfx::MUTED, Action::ClearReply);
    }
    ctx.g.panel(x + 12, cy, cw - 24, composer_h, gfx::RAISED);
    let song_button = c.now_playing_installed() && (!lobby || !c.st.lobby_muted);
    let hint = if lobby && c.st.lobby_muted {
        "You're muted in the community chat".to_owned()
    } else if lobby {
        "Message VCraft Community".to_owned()
    } else if let Some(g) = c.group(conv) {
        format!("Message {}", g.name)
    } else if let Some(f) = c.friend(conv) {
        format!("Message @{}", f.name)
    } else {
        "Message".to_owned()
    };
    let max = if lobby { 300 } else { 500 };
    let value = ctx.field("composer", x + 22, cy + 9, cw - if song_button { 72 } else { 50 }, &hint, max);
    if song_button {
        let song = c.now_playing_current();
        let col = if song.is_some() { gfx::GREEN } else { gfx::FAINT };
        ctx.icon_button(x + cw - 56, cy + 4, 18, 18, &gfx::NOTE, 0, gfx::HOVER, col, Action::SendSong(conv.to_owned()));
        if ctx.inside(x + cw - 56, cy + 4, 18, 18) {
            ctx.tooltip = Some(match song {
                Some(s) => format!("Share what you're listening to: {s}"),
                None => "Nothing is playing".into(),
            });
        }
    }
    let sc = if value.trim().is_empty() { gfx::FAINT } else { gfx::TEXT };
    ctx.icon_button(x + cw - 34, cy + 4, 18, 18, &gfx::RIGHT, 0, gfx::HOVER, sc, Action::SendComposer);
    let top = ey + 16;
    let bottom = cy - if c.ui.reply_to.is_some() { 24 } else { 4 };
    c.ui.chat = [x, top, cw, bottom - top];
    draw_messages(c, ctx, x, top, cw, bottom - top, conv);
    if c.ui.show_pins && !lobby {
        draw_pins(c, ctx, x, top, cw, bottom - top, conv);
    }
}

/// Your pinned messages for this conversation, over the top of the chat.
fn draw_pins(c: &Core, ctx: &mut Ctx, x: i32, top: i32, cw: i32, area_h: i32, conv: &str) {
    let pins = c.pins(conv);
    let rows = pins.len().max(1) as i32;
    let ph = (rows * 24 + 22).min(area_h - 10);
    let (px, pw) = (x + 12, cw - 24);
    ctx.g.panel(px - 1, top + 3, pw + 2, ph + 2, gfx::SHADOW);
    ctx.g.panel(px, top + 4, pw, ph, gfx::MENU);
    // Clicks on the panel don't reach the messages under it.
    ctx.hit(px, top + 4, pw, ph, Action::None);
    ctx.g.icon(&PIN, px + 8, top + 10, gfx::MUTED);
    ctx.g.text(t("YOUR PINS"), px + 22, top + 11, gfx::FAINT);
    ctx.g.text(t("Only you see these."), px + 30 + ctx.g.width(t("YOUR PINS")), top + 11, gfx::FAINT);
    ctx.icon_button(px + pw - 20, top + 7, 16, 14, &gfx::CLOSE, 0, gfx::HOVER, gfx::MUTED, Action::TogglePins);
    ctx.g.scissor(px, top + 24, pw, ph - 22);
    let mut ry = top + 24;
    if pins.is_empty() {
        ctx.g.text(t("Hover a message and click the pin to keep it here."), px + 10, ry + 6, gfx::FAINT);
    }
    for p in pins.iter().rev() {
        ctx.g.text(&p.from_name, px + 10, ry + 2, gfx::WHITE);
        let nw = ctx.g.width(&p.from_name);
        ctx.g.text(&time(p.ts), px + 16 + nw, ry + 2, gfx::FAINT);
        let body = ctx.g.clip(&p.text, pw - 50);
        ctx.g.text(&body, px + 10, ry + 12, gfx::TEXT);
        ctx.icon_button(px + pw - 24, ry + 4, 16, 14, &gfx::CLOSE, 0, gfx::HOVER, gfx::FAINT, Action::Unpin(conv.to_owned(), p.id.clone()));
        ry += 24;
    }
    ctx.g.end_scissor();
}

struct Item {
    msg: state::Message,
    header: bool,
    gap: i32,
    lines: Vec<(String, u32)>,
    height: i32,
}

fn draw_messages(c: &mut Core, ctx: &mut Ctx, x: i32, top: i32, cw: i32, area_h: i32, conv: &str) {
    let msgs = c.st.conversations.get(conv).cloned().unwrap_or_default();
    let text_w = cw - 70;
    let mut items: Vec<Item> = Vec::new();
    let mut total = 0;
    let mut prev: Option<&state::Message> = None;
    for m in &msgs {
        let header = !m.system
            && match prev {
                None => true,
                Some(p) => p.system || m.from != p.from || m.ts - p.ts > 300_000 || m.reply.is_some(),
            };
        let mut lines = Vec::new();
        if m.system {
            lines = ctx.g.split(&m.text, text_w).into_iter().map(|s| (s, gfx::MUTED)).collect();
        } else if m.song.is_none() {
            let color = if m.secure { gfx::TEXT } else { gfx::SOFT_RED };
            lines = ctx.g.split(&m.text, text_w).into_iter().map(|s| (s, color)).collect();
        }
        let gap = if (header || m.system) && prev.is_some() { 6 } else { 0 };
        let height = gap
            + lines.len() as i32 * 10
            + if header { 14 } else { 0 }
            + if m.reply.is_some() { 12 } else { 0 }
            + if m.invite.is_some() { 44 } else { 0 }
            + if m.song.is_some() { 40 } else { 0 }
            + 4;
        items.push(Item { msg: m.clone(), header, gap, lines, height });
        total += height;
        prev = Some(m);
    }
    c.ui.chat_max = 0.max(total - area_h + 8);
    c.ui.chat_scroll = 0.max(c.ui.chat_scroll.min(c.ui.chat_max));
    ctx.g.scissor(x, top, cw, area_h);
    let mut yy = top + area_h - 4 - total + c.ui.chat_scroll;
    if total < area_h - 8 {
        let who = c.conv_name(conv);
        let mut sy = top + 12;
        if conv == LOBBY {
            sy = ctx.g.para("Welcome to the VCraft Community, the official chat for everyone using VCraft.", x + 16, sy, cw - 32, gfx::MUTED);
            ctx.g.para("Say hi, find people to play with, and keep it friendly. Moderators can mute people who aren't.", x + 16, sy + 2, cw - 32, gfx::FAINT);
        } else {
            sy = ctx.g.para(&format!("This is the start of your conversation with {who}."), x + 16, sy, cw - 32, gfx::MUTED);
            ctx.g.para("Messages are end-to-end encrypted: only the people in it can read them.", x + 16, sy + 2, cw - 32, gfx::FAINT);
        }
    }
    for it in &items {
        if yy + it.height >= top && yy <= top + area_h {
            draw_message(c, ctx, x, yy, cw, it, top, area_h, conv);
        }
        yy += it.height;
    }
    ctx.g.end_scissor();
}

#[allow(clippy::too_many_arguments)]
fn draw_message(c: &Core, ctx: &mut Ctx, x: i32, y: i32, cw: i32, it: &Item, clip_top: i32, clip_h: i32, conv: &str) {
    let m = &it.msg;
    let mut yy = y + it.gap;
    if m.system {
        let mut ly = yy + 2;
        for (t, col) in &it.lines {
            ctx.g.text(t, x + 50, ly, *col);
            ly += 10;
        }
        ctx.g.fill(x + 16, yy + 6, 26, 1, gfx::FAINT);
        return;
    }
    let mine = Some(&m.from) == c.st.my_uuid.as_ref();
    let top = y + it.gap - 2;
    let vis_top = top.max(clip_top);
    let vis_bottom = (y + it.height).min(clip_top + clip_h);
    let hover = ctx.inside(x, vis_top, cw, vis_bottom - vis_top) && c.ui.menu.is_none();
    if hover {
        ctx.g.fill(x, top, cw, y + it.height - top, 0xFF2E_3035);
    }
    if let Some(r) = &m.reply {
        ctx.g.fill(x + 30, yy + 5, 12, 1, gfx::FAINT);
        ctx.g.fill(x + 30, yy + 5, 1, 6, gfx::FAINT);
        let t = ctx.g.clip(&format!("{}  {}", r.name, r.text), cw - 120);
        ctx.g.text(&t, x + 50, yy + 1, gfx::FAINT);
        yy += 12;
    }
    if it.header {
        ctx.avatar(x + 14, yy + 2, 26, &m.from, &m.from_name, None);
        let author = if mine { m.from_name.clone() } else { c.cfg.display_name(&m.from, &m.from_name) };
        ctx.g.text(&author, x + 50, yy + 2, if mine { 0xFF7D_D3A0 } else { gfx::WHITE });
        let mut after = x + 54 + ctx.g.width(&author);
        if c.is_pinned(conv, m.id.as_deref().unwrap_or("")) {
            ctx.g.icon(&PIN, after, yy + 1, gfx::GOLD);
            after += 12;
        }
        after += rank_badge(&mut ctx.g, after, yy + 1, c.rank(Some(&m.from)).as_deref());
        ctx.g.text(&time(m.ts), after + 2, yy + 2, gfx::FAINT);
        yy += 14;
    }
    for (t, col) in &it.lines {
        ctx.g.text(t, x + 50, yy, *col);
        yy += 10;
    }
    if let Some(song) = &m.song {
        let card_w = 260.min(cw - 70);
        ctx.g.panel(x + 50, yy + 1, card_w, 36, gfx::PANEL);
        ctx.g.fill(x + 50, yy + 1, 2, 36, gfx::GREEN);
        ctx.g.panel(x + 58, yy + 7, 24, 24, 0xFF1F_3A2A);
        ctx.g.icon(&gfx::NOTE, x + 66, yy + 15, gfx::GREEN);
        ctx.g.text(if mine { "YOU'RE LISTENING TO" } else { "LISTENING TO" }, x + 90, yy + 8, gfx::FAINT);
        let s = ctx.g.clip(song, card_w - 48);
        ctx.g.text(&s, x + 90, yy + 20, gfx::WHITE);
    }
    if let Some(inv) = &m.invite {
        let card_w = 240.min(cw - 70);
        ctx.g.panel(x + 50, yy + 3, card_w, 38, gfx::PANEL);
        ctx.g.fill(x + 50, yy + 3, 2, 38, gfx::GREEN);
        ctx.g.icon(&gfx::SERVER, x + 58, yy + 9, gfx::GREEN);
        let n = ctx.g.clip(&inv.name, card_w - 90);
        ctx.g.text(&n, x + 72, yy + 9, gfx::WHITE);
        let a = ctx.g.clip(&inv.address, card_w - 90);
        ctx.g.text(&a, x + 72, yy + 22, gfx::MUTED);
        if !mine {
            ctx.button(x + 50 + card_w - 54, yy + 12, 46, 18, "Join", gfx::GREEN, gfx::GREEN_HI, gfx::WHITE, Action::PendingJoin(inv.clone(), Some(m.from_name.clone())));
        } else {
            ctx.g.text("Sent", x + 50 + card_w - 34, yy + 16, gfx::FAINT);
        }
    }
    if let (true, Some(id)) = (hover, m.id.clone()) {
        let lobby = conv == LOBBY;
        let can_delete = mine || (lobby && c.is_staff());
        let can_menu = lobby && !mine;
        let can_pin = !lobby;
        let bw = 24 + if can_delete { 22 } else { 0 } + if can_menu { 22 } else { 0 } + if can_pin { 22 } else { 0 };
        let bx = x + cw - bw - 12;
        let by = (top - 6).max(clip_top);
        ctx.g.panel(bx - 1, by - 1, bw + 2, 22, gfx::BG);
        ctx.g.panel(bx, by, bw, 20, gfx::RAISED);
        let reply_text = match &m.invite {
            Some(inv) => format!("Server invite: {}", inv.name),
            None => m.text.clone(),
        };
        let reply = Reply { id: id.clone(), name: m.from_name.clone(), text: reply_text };
        ctx.icon_button(bx + 2, by + 2, 20, 16, &gfx::REPLY, 0, gfx::HOVER, gfx::MUTED, Action::SetReply(reply));
        let mut ax = bx + 24;
        if can_pin {
            let pinned = c.is_pinned(conv, &id);
            ctx.icon_button(ax, by + 2, 20, 16, &PIN, 0, gfx::HOVER, if pinned { gfx::GOLD } else { gfx::MUTED }, Action::TogglePin(conv.to_owned(), id.clone()));
            if ctx.inside(ax, by + 2, 20, 16) {
                ctx.tooltip = Some(if pinned { t("Unpin") } else { t("Pin for yourself") }.to_owned());
            }
            ax += 22;
        }
        if can_delete {
            ctx.icon_button(ax, by + 2, 20, 16, &gfx::TRASH, 0, gfx::RED, gfx::SOFT_RED, Action::DeleteMessage(conv.to_owned(), id.clone()));
            ax += 22;
        }
        if can_menu {
            ctx.icon_button(ax, by + 2, 20, 16, &gfx::DOTS, 0, gfx::HOVER, gfx::MUTED, Action::LobbyUserMenu(m.from.clone(), m.from_name.clone()));
        }
    }
}

fn draw_members(c: &Core, ctx: &mut Ctx, x: i32, y: i32, mw: i32, ph: i32, group: &Group) {
    ctx.g.fill(x, y + 1, mw - 1, ph - 2, gfx::PANEL);
    ctx.g.fill(x, y, 1, ph, gfx::LINE);
    ctx.g.text(&format!("MEMBERS - {}", group.members.len()), x + 10, y + 12, gfx::FAINT);
    let mut my = y + 28;
    for p in &group.members {
        if ctx.inside(x + 4, my, mw - 9, 24) {
            ctx.g.panel(x + 4, my, mw - 9, 24, gfx::HOVER);
        }
        ctx.avatar(x + 8, my + 4, 16, &p.uuid, &p.name, Some(&p.status));
        let label = ctx.g.clip(&p.name, mw - 46);
        ctx.g.text(&label, x + 30, my + 8, if p.online() { gfx::TEXT } else { gfx::FAINT });
        let mut after = x + 33 + ctx.g.width(&label);
        if p.uuid == group.owner {
            ctx.g.text("*", after, my + 8, gfx::GOLD);
            after += 8;
        }
        if after + 30 < x + mw {
            rank_badge(&mut ctx.g, after, my + 7, c.rank(Some(&p.uuid)).as_deref());
        }
        let a = Action::MemberMenu(group.id.clone(), p.uuid.clone());
        ctx.hit2(x + 4, my, mw - 9, 24, a.clone(), a);
        my += 26;
        if my > y + ph - 30 {
            break;
        }
    }
    let hover_add = ctx.inside(x + 4, y + ph - 28, mw - 9, 22);
    if hover_add {
        ctx.g.panel(x + 4, y + ph - 28, mw - 9, 22, gfx::HOVER);
    }
    let col = if hover_add { gfx::WHITE } else { gfx::MUTED };
    ctx.g.icon(&gfx::PLUS, x + 10, y + ph - 21, col);
    ctx.g.text("Add people", x + 26, y + ph - 20, col);
    ctx.hit(x + 4, y + ph - 28, mw - 9, 22, Action::AddToGroupMenu(group.id.clone()));
}

fn draw_new_group(c: &mut Core, ctx: &mut Ctx, x: i32, y: i32, pw: i32, ph: i32) {
    header(ctx, x, y, pw);
    ctx.g.icon(&gfx::PEOPLE, x + 12, y + 12, gfx::MUTED);
    ctx.g.text("Create a Group", x + 28, y + 13, gfx::WHITE);
    let cy = y + 46;
    ctx.g.text("GROUP NAME", x + 16, cy, gfx::MUTED);
    ctx.g.panel(x + 16, cy + 12, pw - 32, 24, gfx::BG);
    let name = ctx.field("groupName", x + 24, cy + 20, pw - 48, "Group name", 32);
    ctx.g.text(&format!("ADD FRIENDS  ({} selected)", c.ui.new_group_members.len()), x + 16, cy + 48, gfx::MUTED);
    set_page_area(c, ctx, x, cy + 62, pw, ph - (cy + 62 - y) - 44);
    let [px, py, pw2, ph2] = ctx.page_area;
    ctx.g.scissor(px, py, pw2, ph2);
    let mut ry = py - c.ui.page_scroll;
    for f in c.st.friends.clone() {
        let sel = c.ui.new_group_members.contains(&f.uuid);
        if ctx.inside(x + 12, ry, pw - 24, 26) {
            ctx.g.panel(x + 12, ry, pw - 24, 26, gfx::HOVER);
        }
        ctx.avatar(x + 18, ry + 4, 18, &f.uuid, &f.name, Some(&f.status));
        ctx.g.text(&f.name, x + 44, ry + 9, gfx::TEXT);
        let bx = x + pw - 36;
        ctx.g.panel(bx, ry + 6, 14, 14, if sel { gfx::GREEN } else { gfx::BG });
        ctx.g.outline(bx, ry + 6, 14, 14, if sel { gfx::GREEN } else { gfx::FAINT });
        if sel {
            ctx.g.icon(&gfx::CHECK, bx + 3, ry + 9, gfx::WHITE);
        }
        ctx.hit(x + 12, ry, pw - 24, 26, Action::ToggleNewGroupMember(f.uuid.clone()));
        ry += 28;
    }
    if c.st.friends.is_empty() {
        ctx.g.text("Add some friends first.", x + 18, ry + 4, gfx::FAINT);
    }
    ctx.g.end_scissor();
    c.ui.page_max = 0.max(ry + c.ui.page_scroll - (py + ph2));
    let ok = !name.trim().is_empty();
    let (bc, bh, bt) = if ok { (gfx::GREEN, gfx::GREEN_HI, gfx::WHITE) } else { (gfx::DISABLED, gfx::DISABLED, gfx::MUTED) };
    ctx.button(x + pw - 116, y + ph - 34, 100, 22, "Create Group", bc, bh, bt, Action::CreateGroup);
    ctx.button(x + pw - 190, y + ph - 34, 66, 22, "Cancel", 0, gfx::HOVER, gfx::TEXT, Action::Go(Page::Friends));
}

fn draw_call(c: &mut Core, ctx: &mut Ctx, x: i32, y: i32, pw: i32, ph: i32) {
    let Some(call) = c.st.call.clone() else { return };
    header(ctx, x, y, pw);
    ctx.g.icon(&gfx::PHONE, x + 12, y + 12, gfx::GREEN);
    ctx.g.text(t("Voice Call"), x + 28, y + 13, gfx::WHITE);
    let (tile_w, tile_h, gap) = (104, 80, 10);
    let per_row = 1.max((pw - 32 + gap) / (tile_w + gap));
    let count = (call.members.len() + call.ringing.len()) as i32;
    let rows = (count + per_row - 1) / per_row;
    let grid_w = per_row.min(count) * (tile_w + gap) - gap;
    let gx = x + (pw - grid_w) / 2;
    let gy = y + 50;
    let me = c.me();
    let mut i = 0;
    for m in &call.members {
        let tx = gx + i % per_row * (tile_w + gap);
        let ty = gy + i / per_row * (tile_h + gap);
        let speaking = crate::voice().is_speaking(&m.uuid);
        let is_me = Some(&m.uuid) == me.as_ref();
        ctx.g.panel(tx, ty, tile_w, tile_h, gfx::MENU);
        if speaking {
            ctx.g.outline(tx, ty, tile_w, tile_h, gfx::GREEN);
        }
        ctx.avatar(tx + tile_w / 2 - 16, ty + 12, 32, &m.uuid, &m.name, None);
        let shown = if is_me { format!("{} ({})", m.name, t("you")) } else { c.cfg.display_name(&m.uuid, &m.name) };
        let label = ctx.g.clip(&shown, tile_w - 8);
        ctx.g.centered(&label, tx + tile_w / 2, ty + 52, gfx::WHITE);
        if m.deafened {
            ctx.g.icon_struck(&gfx::HEADPHONES, tx + tile_w - 14, ty + 4, gfx::MUTED);
        } else if m.muted {
            ctx.g.icon_struck(&gfx::MIC, tx + tile_w - 14, ty + 4, gfx::MUTED);
        }
        let muted_for_me = c.cfg.muted_users.contains(&m.uuid);
        if !is_me {
            let state = if trust(&m.uuid) != Trust::Trusted {
                t("key not trusted")
            } else if !crate::with_secure(|s| s.has_voice_key_from(true, &m.uuid)) {
                t("securing...")
            } else if muted_for_me {
                t("muted for you")
            } else {
                ""
            };
            if !state.is_empty() {
                ctx.g.centered(state, tx + tile_w / 2, ty + 64, if muted_for_me { gfx::FAINT } else { gfx::GOLD });
            }
            let a = Action::MemberMenu(String::new(), m.uuid.clone());
            ctx.hit2(tx, ty, tile_w, tile_h, a.clone(), a);
        }
        i += 1;
    }
    for p in &call.ringing {
        let tx = gx + i % per_row * (tile_w + gap);
        let ty = gy + i / per_row * (tile_h + gap);
        ctx.g.panel(tx, ty, tile_w, tile_h, gfx::MENU);
        ctx.avatar(tx + tile_w / 2 - 16, ty + 12, 32, &p.uuid, &p.name, None);
        ctx.g.fill(tx + tile_w / 2 - 16, ty + 12, 32, 32, 0x8023_2428);
        let n = ctx.g.clip(&p.name, tile_w - 8);
        ctx.g.centered(&n, tx + tile_w / 2, ty + 52, gfx::MUTED);
        ctx.g.centered(t("Ringing..."), tx + tile_w / 2, ty + 64, gfx::FAINT);
        i += 1;
    }
    let mut iy = gy + rows * (tile_h + gap) + 6;
    // People asking to join.
    let asking: Vec<_> = c.social.requests.iter().filter(|r| r.kind == RequestKind::Call).cloned().collect();
    if !asking.is_empty() {
        ctx.g.text(t("ASKING TO JOIN"), x + 16, iy, gfx::GOLD);
        iy += 14;
        for r in asking {
            ctx.g.panel(x + 16, iy, pw - 32, 22, gfx::RAISED);
            ctx.avatar(x + 20, iy + 4, 14, &r.from, &r.name, None);
            ctx.g.text(&r.name, x + 40, iy + 7, gfx::WHITE);
            ctx.button(x + pw - 140, iy + 2, 60, 18, t("Let in"), gfx::GREEN, gfx::GREEN_HI, gfx::WHITE, Action::AcceptRequest(r.from.clone(), r.kind.clone()));
            ctx.button(x + pw - 76, iy + 2, 56, 18, t("Ignore"), 0, gfx::HOVER, gfx::TEXT, Action::DismissRequest(r.from.clone(), r.kind.clone()));
            iy += 26;
        }
        iy += 4;
    }
    ctx.g.text(t("INVITE TO CALL"), x + 16, iy, gfx::MUTED);
    let (mut ix, mut iyy) = (x + 16, iy + 14);
    let mut candidates: Vec<Profile> = c.st.friends.iter().filter(|f| f.online() && !c.in_call(&f.uuid)).cloned().collect();
    for g in &c.st.groups {
        for p in &g.members {
            if p.online() && !c.in_call(&p.uuid) && Some(&p.uuid) != me.as_ref() && !candidates.iter().any(|x| x.uuid == p.uuid) {
                candidates.push(p.clone());
            }
        }
    }
    for p in &candidates {
        let name = c.cfg.display_name(&p.uuid, &p.name);
        let cw = ctx.g.width(&name) + 34;
        if ix + cw > x + pw - 16 {
            ix = x + 16;
            iyy += 24;
        }
        if iyy > y + ph - 70 {
            break;
        }
        let hover = ctx.inside(ix, iyy, cw, 20);
        ctx.g.panel(ix, iyy, cw, 20, if hover { gfx::HOVER } else { gfx::RAISED });
        ctx.avatar(ix + 4, iyy + 4, 12, &p.uuid, &p.name, None);
        ctx.g.text(&name, ix + 20, iyy + 6, gfx::TEXT);
        ctx.g.icon(&gfx::PLUS, ix + cw - 12, iyy + 6, if hover { gfx::GREEN_HI } else { gfx::MUTED });
        ctx.hit(ix, iyy, cw, 20, Action::Call(p.uuid.clone()));
        ix += cw + 6;
    }
    if candidates.is_empty() {
        ctx.g.text(t("Nobody else is online."), x + 16, iyy + 4, gfx::FAINT);
    }
    let by = y + ph - 46;
    let bx = x + pw / 2 - 87;
    big_toggle(ctx, bx, by, &gfx::MIC, c.st.muted || c.st.deafened, Action::ToggleMute);
    big_toggle(ctx, bx + 46, by, &gfx::HEADPHONES, c.st.deafened, Action::ToggleDeafen);
    ctx.icon_button(bx + 92, by, 40, 32, &gfx::REFRESH, gfx::RAISED, gfx::HOVER, gfx::WHITE, Action::ReconnectVoice);
    ctx.icon_button(bx + 138, by, 40, 32, &gfx::HANGUP, gfx::RED, gfx::RED_HI, gfx::WHITE, Action::Relay("call_leave", None, None));
    if ctx.inside(bx + 92, by, 40, 32) {
        ctx.g.centered(t("Reconnect voice"), bx + 112, by - 12, gfx::MUTED);
    }
}

fn toggle_row(ctx: &mut Ctx, lx: i32, cx: i32, y: i32, label: &str, on: bool, action: Action) -> i32 {
    ctx.g.text(label, lx, y + 5, gfx::TEXT);
    let hover = ctx.inside(cx, y + 2, 30, 14);
    let c = if on { if hover { gfx::GREEN_HI } else { gfx::GREEN } } else if hover { gfx::FAINT } else { 0xFF4E_5058 };
    ctx.g.panel(cx, y + 2, 30, 14, c);
    ctx.g.panel(if on { cx + 18 } else { cx + 2 }, y + 4, 10, 10, gfx::WHITE);
    ctx.hit(cx, y + 2, 30, 14, action);
    y + 22
}

fn segment(ctx: &mut Ctx, x: i32, y: i32, sw: i32, label: &str, sel: bool, action: Action) {
    let hover = ctx.inside(x, y, sw, 18);
    ctx.g.panel(x, y, sw, 18, if sel { gfx::SELECTED } else if hover { gfx::HOVER } else { gfx::RAISED });
    let l = ctx.g.clip(label, sw - 6);
    ctx.g.centered(&l, x + sw / 2, y + 5, if sel { gfx::WHITE } else { gfx::MUTED });
    ctx.hit(x, y, sw, 18, action);
}

#[allow(clippy::too_many_arguments)]
fn slider_row(ctx: &mut Ctx, lx: i32, cx: i32, cw: i32, y: i32, label: &str, value: f64, shown: &str, which: Slider) -> i32 {
    ctx.g.text(label, lx, y + 5, gfx::TEXT);
    let track = cw - 46;
    let v = value.clamp(0.0, 1.0);
    ctx.g.panel(cx, y + 7, track, 4, 0xFF4E_5058);
    ctx.g.panel(cx, y + 7, (track as f64 * v) as i32, 4, gfx::GREEN);
    let kx = cx + (track as f64 * v) as i32 - 4;
    ctx.g.panel(kx, y + 3, 9, 12, gfx::WHITE);
    ctx.g.text(shown, cx + track + 8, y + 5, gfx::MUTED);
    ctx.hit(cx - 4, y, track + 8, 18, Action::Slide(which));
    y + 22
}

fn slider_geometry(c: &Core) -> (i32, i32) {
    let [w0, _] = c.ui.size;
    let w = (w0 - 16).min(720);
    let x0 = (w0 - w) / 2;
    let side = 200.min(150.max(w / 3));
    let (x, pw) = (x0 + side, w - side);
    let cx = x + pw / 2;
    let cw = pw / 2 - 18;
    (cx, cw - 46)
}

fn set_slider(c: &mut Core, which: Slider, mouse_x: i32) {
    let (cx, track) = slider_geometry(c);
    let v = ((mouse_x - cx) as f64 / track.max(1) as f64).clamp(0.0, 1.0);
    match which {
        Slider::Threshold => c.cfg.voice_threshold_db = (-70.0 + v * 60.0).round(),
        Slider::MicVolume => c.cfg.mic_volume = (v * 200.0).round() / 100.0,
        Slider::OutputVolume => c.cfg.output_volume = (v * 200.0).round() / 100.0,
    }
    c.push_voice_params();
}

fn draw_settings(c: &mut Core, ctx: &mut Ctx, x: i32, y: i32, pw: i32, ph: i32) {
    header(ctx, x, y, pw);
    ctx.g.icon(&gfx::GEAR, x + 12, y + 12, gfx::MUTED);
    ctx.g.text(t("Settings"), x + 28, y + 13, gfx::WHITE);
    set_page_area(c, ctx, x, y + 34, pw, ph - 36);
    let [px, py, pw2, ph2] = ctx.page_area;
    ctx.g.scissor(px, py, pw2, ph2);
    let mut sy = py + 10 - c.ui.page_scroll;
    let lx = x + 18;
    let cx = x + pw / 2;
    let cw = pw / 2 - 18;
    sy = ctx.section(t("ACCOUNT"), lx, sy, pw - 36);
    ctx.g.text(t("Status"), lx, sy + 5, gfx::TEXT);
    let seg_w = cw / 4;
    for (i, label) in [t("Online"), t("Idle"), t("DND"), t("Offline")].iter().enumerate() {
        let s = state::STATUSES[i];
        let sel = c.st.my_status == s;
        let sx = cx + i as i32 * seg_w;
        let hover = ctx.inside(sx, sy, seg_w - 2, 18);
        ctx.g.panel(sx, sy, seg_w - 2, 18, if sel { gfx::SELECTED } else if hover { gfx::HOVER } else { gfx::RAISED });
        ctx.g.fill(sx + 5, sy + 7, 4, 4, state::status_color(Some(s)));
        let l = ctx.g.clip(label, seg_w - 16);
        ctx.g.text(&l, sx + 12, sy + 5, if sel { gfx::WHITE } else { gfx::MUTED });
        ctx.hit(sx, sy, seg_w - 2, 18, Action::SetStatus(s.into()));
    }
    sy += 24;
    sy = ctx.g.para(t("Do Not Disturb silences pop-ups, sounds and ringing. Offline hides you."), lx, sy, pw - 36, gfx::FAINT) + 8;
    ctx.g.text(t("Language"), lx, sy + 5, gfx::TEXT);
    ctx.icon_button(cx, sy, 18, 18, &gfx::LEFT, gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::CycleLanguage(false));
    ctx.g.panel(cx + 20, sy, cw - 42, 18, gfx::BG);
    let lang = if c.cfg.language == "game" { t("Same as the game").to_owned() } else { crate::i18n::name(&c.cfg.language).to_owned() };
    let lang = ctx.g.clip(&lang, cw - 50);
    ctx.g.centered(&lang, cx + 20 + (cw - 42) / 2, sy + 5, gfx::WHITE);
    ctx.icon_button(cx + cw - 20, sy, 18, 18, &gfx::RIGHT, gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::CycleLanguage(true));
    sy += 26;
    sy = ctx.section(t("CONNECTION"), lx, sy, pw - 36);
    ctx.g.text(t("Relay address"), lx, sy + 5, gfx::TEXT);
    ctx.g.panel(cx, sy, cw - 52, 18, gfx::BG);
    if !ctx.fields.contains_key("relayUrl") {
        ctx.set_field("relayUrl", &c.cfg.server_url);
    }
    ctx.field("relayUrl", cx + 6, sy + 5, cw - 64, "wss://relay.example.com", 256);
    ctx.button(cx + cw - 48, sy, 48, 18, t("Save"), gfx::GREEN, gfx::GREEN_HI, gfx::WHITE, Action::SaveRelay);
    sy += 22;
    let st = crate::relay().status();
    let st_text = match st {
        Status::Connected => tf("Connected as {0}", &[&c.st.my_name.clone().unwrap_or_default()]),
        Status::Connecting => t("Connecting...").into(),
        Status::Authenticating => t("Signing in...").into(),
        Status::Disconnected => crate::relay().last_error().unwrap_or_else(|| t("Not connected").into()),
    };
    let st_text = ctx.g.clip(&st_text, cw);
    let st_col = match st {
        Status::Connected => gfx::GREEN,
        Status::Disconnected => gfx::RED,
        _ => gfx::GOLD,
    };
    ctx.g.text(&st_text, cx, sy + 2, st_col);
    sy += 16;
    sy = toggle_row(ctx, lx, cx, sy, t("Connect on startup"), c.cfg.auto_connect, Action::SetToggle(Toggle::AutoConnect));
    sy = ctx.section(t("ABOUT"), lx, sy + 4, pw - 36);
    ctx.g.text(&tf("VCraft {0} for Horizon", &[crate::VERSION]), lx, sy + 5, gfx::TEXT);
    sy += 18;
    sy = ctx.g.para(t("This is a Horizon native mod, so it updates through the Horizon launcher, not by itself."), lx, sy, pw - 36, gfx::FAINT) + 6;
    sy = ctx.section(t("VOICE"), lx, sy + 4, pw - 36);
    ctx.g.text(t("Input mode"), lx, sy + 5, gfx::TEXT);
    let half = cw / 2;
    segment(ctx, cx, sy, half - 2, t("Voice Activity"), !c.cfg.push_to_talk, Action::SetPushToTalk(false));
    segment(ctx, cx + half, sy, half - 2, t("Push to Talk"), c.cfg.push_to_talk, Action::SetPushToTalk(true));
    sy += 24;
    let th = c.cfg.voice_threshold_db;
    sy = slider_row(ctx, lx, cx, cw, sy, t("Activation threshold"), (th + 70.0) / 60.0, &format!("{} dB", th.round() as i64), Slider::Threshold);
    let voice = crate::voice();
    if voice.running() {
        let lvl = voice.mic_level_db().clamp(-70.0, -10.0);
        let level = (lvl as f64 + 70.0) / 60.0;
        let over = voice.mic_level_db() as f64 >= th;
        ctx.g.fill(cx, sy - 6, (cw as f64 * level) as i32, 3, if over { gfx::GREEN } else { gfx::FAINT });
    }
    let mv = c.cfg.mic_volume;
    sy = slider_row(ctx, lx, cx, cw, sy, t("Mic volume"), mv / 2.0, &format!("{}%", (mv * 100.0).round() as i64), Slider::MicVolume);
    let ov = c.cfg.output_volume;
    sy = slider_row(ctx, lx, cx, cw, sy, t("Voice volume"), ov / 2.0, &format!("{}%", (ov * 100.0).round() as i64), Slider::OutputVolume);
    sy = toggle_row(ctx, lx, cx, sy, t("Noise suppression"), c.cfg.noise_suppression, Action::SetToggle(Toggle::NoiseSuppression));
    sy = toggle_row(ctx, lx, cx, sy, t("Echo cancellation"), c.cfg.echo_cancellation, Action::SetToggle(Toggle::EchoCancellation));
    sy = ctx.g.para(t("Turn this on if you use speakers instead of headphones, so others don't hear themselves."), lx, sy - 2, pw - 36, gfx::FAINT) + 6;
    ctx.g.text(t("Voice effect"), lx, sy + 5, gfx::TEXT);
    ctx.icon_button(cx, sy, 18, 18, &gfx::LEFT, gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::CycleEffect(false));
    ctx.g.panel(cx + 20, sy, cw - 42, 18, gfx::BG);
    let fx_name = t(crate::effects::label(&c.cfg.voice_effect));
    ctx.g.centered(fx_name, cx + 20 + (cw - 42) / 2, sy + 5, if c.cfg.voice_effect == "none" { gfx::MUTED } else { gfx::GREEN });
    ctx.icon_button(cx + cw - 20, sy, 18, 18, &gfx::RIGHT, gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::CycleEffect(true));
    sy += 24;
    ctx.g.text(t("Mic test"), lx, sy + 5, gfx::TEXT);
    let testing = voice.mic_test();
    let (b1, b2) = if testing { (gfx::RED, gfx::RED_HI) } else { (gfx::RAISED, gfx::HOVER) };
    ctx.button(cx, sy, 110, 18, if testing { t("Stop testing") } else { t("Let's check") }, b1, b2, gfx::WHITE, Action::MicTest);
    if let Some(e) = voice.error() {
        let e = ctx.g.clip(&e, cw - 116);
        ctx.g.text(&e, cx + 116, sy + 5, gfx::RED);
    }
    sy += 26;
    sy = toggle_row(ctx, lx, cx, sy, t("Mute while paused"), c.cfg.auto_mute_paused, Action::SetToggle(Toggle::AutoMutePaused));
    sy = toggle_row(ctx, lx, cx, sy, t("Mute in other windows"), c.cfg.auto_mute_unfocused, Action::SetToggle(Toggle::AutoMuteUnfocused));
    sy = ctx.section(t("CALLS"), lx, sy + 2, pw - 36);
    sy = toggle_row(ctx, lx, cx, sy, t("Rejoin calls after a dropped connection"), c.cfg.auto_rejoin, Action::SetToggle(Toggle::AutoRejoin));
    sy = toggle_row(ctx, lx, cx, sy, t("Show friends my server and calls"), c.cfg.share_server, Action::SetToggle(Toggle::ShareServer));
    sy = ctx.g.para(t("Friends on VCraft for Horizon see which server you're on and when you're in a call, and can ask to join. End-to-end encrypted; hidden while you're Invisible."), lx, sy - 2, pw - 36, gfx::FAINT) + 8;
    sy = ctx.section(t("NOTIFICATIONS"), lx, sy, pw - 36);
    sy = toggle_row(ctx, lx, cx, sy, t("Pop-up notifications"), c.cfg.notifications, Action::SetToggle(Toggle::Notifications));
    sy = toggle_row(ctx, lx, cx, sy, t("Message sound"), c.cfg.message_sound, Action::SetToggle(Toggle::MessageSound));
    sy = ctx.section(t("HUD"), lx, sy, pw - 36);
    sy = toggle_row(ctx, lx, cx, sy, t("Compact call HUD (faces only)"), c.cfg.hud_compact, Action::SetToggle(Toggle::HudCompact));
    sy = toggle_row(ctx, lx, cx, sy, t("Unread badge while playing"), c.cfg.hud_unread, Action::SetToggle(Toggle::HudUnread));
    sy = ctx.section(t("NOW LISTENING"), lx, sy, pw - 36);
    sy = toggle_row(ctx, lx, cx, sy, t("Show friends my song"), c.cfg.share_listening, Action::SetToggle(Toggle::ShareListening));
    let np = if c.now_playing_installed() {
        t("Friends see what you're playing on your computer, end-to-end encrypted. Hidden while you're Invisible or when Now Playing's sharing is off.")
    } else {
        t("Turn on the Now Playing mod to show friends what you're listening to and share songs in chat.")
    };
    sy = ctx.g.para(np, lx, sy - 2, pw - 36, gfx::FAINT) + 8;
    ctx.g.text(t("Ringtone"), lx, sy + 5, gfx::TEXT);
    let ring_on = c.cfg.ringtone != "none";
    ctx.icon_button(cx, sy, 18, 18, &gfx::LEFT, gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::CycleRingtone);
    ctx.g.panel(cx + 20, sy, cw - 62, 18, gfx::BG);
    let rn = ctx.g.clip(if ring_on { t("Bell chime") } else { t("None (silent)") }, cw - 70);
    ctx.g.centered(&rn, cx + 20 + (cw - 62) / 2, sy + 5, gfx::WHITE);
    ctx.icon_button(cx + cw - 40, sy, 18, 18, &gfx::RIGHT, gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::CycleRingtone);
    let prev = c.ui.previewing;
    let (pi, pc) = if prev { (&gfx::STOP, gfx::RED) } else { (&gfx::PLAY, gfx::GREEN) };
    ctx.icon_button(cx + cw - 18, sy, 18, 18, pi, gfx::RAISED, gfx::HOVER, pc, Action::PreviewRingtone);
    sy += 22;
    sy = ctx.g.para(t("Horizon can't stop a music disc once it starts, so calls ring with a repeating bell instead."), lx, sy, pw - 36, gfx::FAINT) + 8;
    sy = ctx.section(t("PRIVACY"), lx, sy, pw - 36);
    ctx.g.text(t("Blocked users"), lx, sy + 5, gfx::TEXT);
    ctx.button(cx, sy, cw.min(130), 18, &tf("{0} blocked - manage", &[&c.st.blocked.len().to_string()]), gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::BlockedTab);
    sy += 24;
    ctx.g.text(t("Your data"), lx, sy + 5, gfx::TEXT);
    let data_w = (cw / 2 - 2).min(120);
    ctx.button(cx, sy, data_w, 18, t("Download my data"), gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::ExportData);
    ctx.button(cx + data_w + 4, sy, data_w, 18, t("Delete account"), 0xFF5C_2B2D, gfx::RED, gfx::PINK_RED, Action::OpenModal(Modal::DeleteAccount, None));
    sy += 24;
    sy = ctx.g.para(t("Messages and calls are end-to-end encrypted. Your key lives in config/vcraft - back it up to keep your identity on a new PC."), lx, sy, pw - 36, gfx::FAINT) + 10;
    sy = ctx.section(t("CONTROLS"), lx, sy, pw - 36);
    for (name, label, _) in crate::KEYS {
        ctx.g.text(t(label), lx, sy + 3, gfx::TEXT);
        let bound = crate::key_label(&c.key_bound(name));
        let unbound = c.key_bound(name) == "key.keyboard.unknown";
        ctx.g.panel(cx, sy, cw.min(120), 14, gfx::BG);
        ctx.g.centered(&bound, cx + cw.min(120) / 2, sy + 3, if unbound { gfx::FAINT } else { gfx::WHITE });
        sy += 17;
    }
    sy = ctx.g.para(t("Change them in Options > Mods > VCraft."), lx, sy + 2, pw - 36, gfx::FAINT) + 2;
    sy = ctx.g.para(t("Commands: /vc msg, call, join, invite, accept, decline, hangup, mute, deafen, status, add, nick"), lx, sy, pw - 36, gfx::FAINT) + 10;
    sy = ctx.section(t("TROUBLESHOOTING"), lx, sy, pw - 36);
    sy = toggle_row(ctx, lx, cx, sy, t("Debug log"), c.cfg.debug_log, Action::SetToggle(Toggle::DebugLog));
    ctx.g.text(t("Log file"), lx, sy + 5, gfx::TEXT);
    ctx.button(cx, sy, cw.min(130), 18, t("Open log folder"), gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::OpenLogFolder);
    sy += 24;
    sy = ctx.g.para(t("config/vcraft/debug.log records connections, audio devices and calls, never what anyone says or types. Attach it to bug reports."), lx, sy, pw - 36, gfx::FAINT) + 12;
    if c.is_staff() {
        let owner = c.st.my_rank.as_deref() == Some("owner");
        let t = format!(
            "Admin ({}): /vc admin stats, list, kick, mute, unmute, clear{}",
            if owner { "owner" } else { "mod" },
            if owner { ", ban, unban, rank, delete" } else { "" }
        );
        sy = ctx.g.para(&t, lx, sy - 8, pw - 36, gfx::GOLD) + 12;
    }
    ctx.g.end_scissor();
    c.ui.page_max = 0.max(sy + c.ui.page_scroll - (py + ph2));
}

fn draw_disconnected(c: &mut Core, ctx: &mut Ctx, x: i32, y: i32, pw: i32, ph: i32) {
    let st = crate::relay().status();
    let cx = x + pw / 2;
    let cy = y + ph / 2 - 40;
    ctx.g.panel(cx - 22, cy - 30, 44, 44, gfx::PANEL);
    ctx.g.icon(&gfx::HEADPHONES, cx - 4, cy - 12, if st == Status::Disconnected { gfx::RED } else { gfx::GOLD });
    let title = match st {
        Status::Connecting => t("Connecting to VCraft..."),
        Status::Authenticating => t("Signing in with your Minecraft account..."),
        _ => t("You're not connected"),
    };
    ctx.g.centered(title, cx, cy + 24, gfx::WHITE);
    if st == Status::Disconnected {
        if let Some(err) = crate::relay().last_error() {
            let e = ctx.g.clip(&err, pw - 40);
            ctx.g.centered(&e, cx, cy + 38, gfx::MUTED);
        }
        let r = ctx.g.clip(&format!("Relay: {}", c.cfg.server_url), pw - 40);
        ctx.g.centered(&r, cx, cy + 50, gfx::FAINT);
        ctx.button(cx - 84, cy + 66, 80, 20, t("Retry"), gfx::GREEN, gfx::GREEN_HI, gfx::WHITE, Action::SaveRelay);
        ctx.button(cx + 4, cy + 66, 80, 20, t("Settings"), gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::Go(Page::Settings));
    }
}

fn draw_notice(c: &Core, ctx: &mut Ctx, main_x: i32, main_w: i32, y0: i32) {
    let Some((text, error, at)) = c.st.notice.clone() else { return };
    if now_ms() - at > 5000 {
        return;
    }
    let text = ctx.g.clip(&text, main_w - 60);
    let nw = ctx.g.width(&text) + 20;
    let nx = main_x + (main_w - nw) / 2;
    let ny = y0 + 40;
    ctx.g.panel(nx, ny, nw, 18, if error { 0xF0B5_3A3D } else { 0xF024_8046 });
    ctx.g.text(&text, nx + 10, ny + 5, gfx::WHITE);
}

fn draw_modal(c: &mut Core, ctx: &mut Ctx, width: i32, height: i32) {
    let Some(modal) = c.ui.modal else { return };
    ctx.g.fill(0, 0, width, height, 0xA000_0000);
    let mw = 320.min(width - 40);
    let mh = match modal {
        Modal::Safety => 128,
        Modal::JoinServer | Modal::DeleteAccount => 112,
        Modal::RenameGroup => 96,
        Modal::FriendNote => 150,
    };
    let mx = (width - mw) / 2;
    let my = (height - mh) / 2;
    ctx.g.panel(mx, my, mw, mh, gfx::MAIN);
    match modal {
        Modal::JoinServer => {
            let Some(inv) = c.ui.pending_join.clone() else {
                c.ui.modal = None;
                return;
            };
            let n = ctx.g.clip(&inv.name, mw - 70);
            ctx.g.text(&format!("Join {n}?"), mx + 16, my + 14, gfx::WHITE);
            let who = c.ui.modal_subject.as_ref().map(|s| format!("{s} invited you to this server. ")).unwrap_or_default();
            ctx.g.para(&format!("{who}Only join servers you trust: the server will see your IP address."), mx + 16, my + 30, mw - 32, gfx::MUTED);
            ctx.g.panel(mx + 16, my + 58, mw - 32, 18, gfx::BG);
            let a = ctx.g.clip(&inv.address, mw - 48);
            ctx.g.text(&a, mx + 22, my + 63, gfx::WHITE);
            ctx.button(mx + mw - 86, my + mh - 30, 70, 20, "Join", gfx::GREEN, gfx::GREEN_HI, gfx::WHITE, Action::Join(inv));
            ctx.button(mx + mw - 160, my + mh - 30, 66, 20, "Cancel", 0, gfx::HOVER, gfx::TEXT, Action::CloseModal);
        }
        Modal::DeleteAccount => {
            ctx.g.text("Delete your VCraft account?", mx + 16, my + 14, gfx::WHITE);
            ctx.g.para(
                "This permanently removes your friends, groups and message history from the relay. Your friends will no longer see you. This can't be undone.",
                mx + 16,
                my + 30,
                mw - 32,
                gfx::MUTED,
            );
            ctx.button(mx + mw - 126, my + mh - 30, 110, 20, "Delete account", gfx::RED, gfx::RED_HI, gfx::WHITE, Action::DeleteAccount);
            ctx.button(mx + mw - 200, my + mh - 30, 66, 20, "Cancel", 0, gfx::HOVER, gfx::TEXT, Action::CloseModal);
        }
        Modal::FriendNote => {
            let subject = c.ui.modal_subject.clone().unwrap_or_default();
            let real = c.friend(&subject).map(|f| f.name.clone()).unwrap_or_default();
            ctx.g.text(&tf("Nickname and note for {0}", &[&real]), mx + 16, my + 14, gfx::WHITE);
            ctx.g.text(t("Only you see these."), mx + 16, my + 26, gfx::FAINT);
            ctx.g.text(t("NICKNAME"), mx + 16, my + 42, gfx::MUTED);
            ctx.g.panel(mx + 16, my + 52, mw - 32, 20, gfx::BG);
            ctx.field("nick", mx + 22, my + 58, mw - 44, &real, 24);
            ctx.g.text(t("NOTE"), mx + 16, my + 78, gfx::MUTED);
            ctx.g.panel(mx + 16, my + 88, mw - 32, 20, gfx::BG);
            ctx.field("note", mx + 22, my + 94, mw - 44, t("Anything you want to remember"), 160);
            ctx.button(mx + mw - 86, my + mh - 30, 70, 20, t("Save"), gfx::GREEN, gfx::GREEN_HI, gfx::WHITE, Action::SaveNote);
            ctx.button(mx + mw - 160, my + mh - 30, 66, 20, t("Cancel"), 0, gfx::HOVER, gfx::TEXT, Action::CloseModal);
        }
        Modal::RenameGroup => {
            ctx.g.text("Rename group", mx + 16, my + 14, gfx::WHITE);
            ctx.g.panel(mx + 16, my + 32, mw - 32, 22, gfx::BG);
            ctx.field("rename", mx + 24, my + 39, mw - 48, "New name", 32);
            ctx.button(mx + mw - 86, my + mh - 30, 70, 20, "Save", gfx::GREEN, gfx::GREEN_HI, gfx::WHITE, Action::SaveRename);
            ctx.button(mx + mw - 160, my + mh - 30, 66, 20, "Cancel", 0, gfx::HOVER, gfx::TEXT, Action::CloseModal);
        }
        Modal::Safety => {
            let subject = c.ui.modal_subject.clone().unwrap_or_default();
            let name = c.conv_name(&subject);
            let tr = trust(&subject);
            let number = crate::with_secure(|s| s.safety_number(&subject));
            ctx.g.text(&format!("Verify {name}"), mx + 16, my + 14, gfx::WHITE);
            ctx.g.text(&format!("Compare this number with {name} over voice."), mx + 16, my + 30, gfx::MUTED);
            ctx.g.text("If it matches on both screens, nobody can listen in.", mx + 16, my + 41, gfx::MUTED);
            ctx.g.panel(mx + 16, my + 56, mw - 32, 24, gfx::BG);
            ctx.g.centered(number.as_deref().unwrap_or("No key yet"), mx + mw / 2, my + 64, if tr == Trust::Changed { gfx::GOLD } else { gfx::WHITE });
            if tr == Trust::Changed {
                ctx.g.text("Their key changed since you last talked.", mx + 16, my + 86, gfx::RED);
                ctx.button(mx + mw - 126, my + mh - 30, 110, 20, "Trust new key", gfx::GREEN, gfx::GREEN_HI, gfx::WHITE, Action::TrustNewKey(subject));
            }
            ctx.button(mx + 16, my + mh - 30, 66, 20, "Close", gfx::RAISED, gfx::HOVER, gfx::TEXT, Action::CloseModal);
        }
    }
}

fn draw_addon(c: &mut Core, ctx: &mut Ctx, x: i32, y: i32, pw: i32, ph: i32) {
    let Some(id) = c.ui.addon_page.clone() else {
        go(c, Page::Friends);
        return;
    };
    let Some(p) = addons::pages(c).into_iter().find(|p| p.id == id) else {
        go(c, Page::Friends);
        return;
    };
    header(ctx, x, y, pw);
    ctx.g.icon(&p.icon, x + 12, y + 12, gfx::MUTED);
    let t = ctx.g.clip(&p.title, pw - 40);
    ctx.g.text(&t, x + 28, y + 13, gfx::WHITE);
    set_page_area(c, ctx, x, y + 34, pw, ph - 36);
    let [px, py, pw2, ph2] = ctx.page_area;
    ctx.g.scissor(px, py, pw2, ph2);
    ctx.content = 0;
    addons::render_page(c, ctx, &id, px + 16, py + 10 - c.ui.page_scroll, pw2 - 32, ph2 - 10);
    ctx.g.end_scissor();
    c.ui.page_max = 0.max(ctx.content + 20 - ph2);
}

fn menu(c: &mut Core, x: i32, y: i32, items: Vec<MenuItem>) {
    c.ui.menu = Some((items, x, y));
}

fn item(label: &str, color: u32, action: Action) -> MenuItem {
    MenuItem { label: label.to_owned(), color, action: Some(action) }
}

fn status_menu(c: &mut Core, x: i32, y: i32) {
    let items = state::STATUSES
        .iter()
        .map(|s| item(&state::status_label(Some(s)).replace(" (invisible)", ""), state::status_color(Some(s)), Action::SetStatus((*s).into())))
        .collect();
    menu(c, x, y, items);
}

fn conv_menu(c: &mut Core, conv: &str, x: i32, y: i32) {
    let mut items = Vec::new();
    if let Some(g) = c.group(conv).cloned() {
        let muted = c.group_muted(conv);
        items.push(item(t("Open chat"), gfx::TEXT, Action::OpenChat(conv.into())));
        items.push(item(t("Call group"), gfx::TEXT, Action::CallGroup(conv.into())));
        items.push(item(t("Invite to this server"), gfx::TEXT, Action::InviteToServer(conv.into())));
        items.push(item(t("Add people"), gfx::TEXT, Action::AddToGroupMenu(g.id.clone())));
        items.push(item(t("Copy invite code"), gfx::TEXT, Action::CopyGroupCode(conv.into())));
        items.push(item(if muted { t("Unmute group") } else { t("Mute group") }, gfx::TEXT, Action::ToggleGroupMute(g.id.clone())));
        items.push(item(t("Rename group"), gfx::TEXT, Action::OpenModal(Modal::RenameGroup, Some(conv.into()))));
        items.push(item(t("Leave group"), gfx::RED, Action::GroupAction("group_leave", g.id.clone(), None, None)));
    } else {
        if c.friend(conv).is_none() {
            return;
        }
        let muted = c.cfg.muted_users.contains(conv);
        items.push(item(t("Message"), gfx::TEXT, Action::OpenChat(conv.into())));
        items.push(item(t("Call"), gfx::TEXT, Action::Call(conv.into())));
        if c.friend_in_call(conv) && c.st.call.is_none() {
            items.push(item(t("Ask to join their call"), gfx::GREEN, Action::AskToJoin(conv.into())));
        }
        items.push(item(t("Invite to this server"), gfx::TEXT, Action::InviteToServer(conv.into())));
        items.push(item(t("Nickname and note"), gfx::TEXT, Action::OpenModal(Modal::FriendNote, Some(conv.into()))));
        items.push(item(t("Verify encryption"), gfx::TEXT, Action::OpenModal(Modal::Safety, Some(conv.into()))));
        items.push(item(if muted { t("Unmute") } else { t("Mute") }, gfx::TEXT, Action::ToggleUserMute(conv.into())));
        items.push(item(t("Remove friend"), gfx::RED, Action::Relay("friend_remove", Some("uuid"), Some(conv.into()))));
        items.push(item(t("Block"), gfx::RED, Action::Relay("block", Some("uuid"), Some(conv.into()))));
    }
    menu(c, x, y, items);
}

fn member_menu(c: &mut Core, group_id: &str, uuid: &str, x: i32, y: i32) {
    if Some(uuid) == c.st.my_uuid.as_deref() {
        return;
    }
    // From the call page: just mute and block.
    if group_id.is_empty() {
        let muted = c.cfg.muted_users.contains(uuid);
        let items = vec![
            item(if muted { t("Unmute for me") } else { t("Mute for me") }, gfx::TEXT, Action::ToggleUserMute(uuid.into())),
            item(t("Block"), gfx::RED, Action::Relay("block", Some("uuid"), Some(uuid.into()))),
        ];
        menu(c, x, y, items);
        return;
    }
    let Some(g) = c.group(&format!("g:{group_id}")).cloned() else { return };
    let Some(p) = g.members.iter().find(|p| p.uuid == uuid).cloned() else { return };
    let mut items = Vec::new();
    if c.friend(uuid).is_some() {
        items.push(item("Message", gfx::TEXT, Action::OpenChat(uuid.into())));
    } else {
        items.push(item("Add friend", gfx::TEXT, Action::Relay("friend_request", Some("name"), Some(p.name.clone()))));
    }
    if p.online() {
        items.push(item("Call", gfx::TEXT, Action::Call(uuid.into())));
    }
    let muted = c.cfg.muted_users.contains(uuid);
    items.push(item(if muted { "Unmute" } else { "Mute" }, gfx::TEXT, Action::ToggleUserMute(uuid.into())));
    if c.st.my_uuid.as_deref() == Some(g.owner.as_str()) {
        items.push(item("Remove from group", gfx::RED, Action::GroupAction("group_kick", g.id.clone(), Some("uuid"), Some(uuid.into()))));
    }
    items.push(item("Block", gfx::RED, Action::Relay("block", Some("uuid"), Some(uuid.into()))));
    menu(c, x, y, items);
}

fn add_to_group_menu(c: &mut Core, group_id: &str, x: i32, y: i32) {
    let Some(g) = c.group(&format!("g:{group_id}")).cloned() else { return };
    let mut items: Vec<MenuItem> = c
        .st
        .friends
        .iter()
        .filter(|f| !g.members.iter().any(|m| m.uuid == f.uuid))
        .map(|f| item(&format!("Add {}", f.name), gfx::TEXT, Action::GroupAction("group_add", g.id.clone(), Some("uuid"), Some(f.uuid.clone()))))
        .collect();
    if items.is_empty() {
        items.push(MenuItem { label: "All your friends are already here".into(), color: gfx::FAINT, action: None });
    }
    menu(c, x, y, items);
}

fn lobby_user_menu(c: &mut Core, uuid: &str, name: &str, x: i32, y: i32) {
    let mut items = vec![MenuItem { label: name.into(), color: gfx::FAINT, action: None }];
    if c.friend(uuid).is_some() {
        items.push(item("Message", gfx::TEXT, Action::OpenChat(uuid.into())));
    } else {
        items.push(item("Add friend", gfx::TEXT, Action::Relay("friend_request", Some("name"), Some(name.into()))));
    }
    if c.is_staff() && c.rank(Some(uuid)).as_deref() != Some("owner") {
        items.push(item("Mute in community", gfx::GOLD, Action::LobbyMute(uuid.into(), true)));
        items.push(item("Unmute in community", gfx::TEXT, Action::LobbyMute(uuid.into(), false)));
    }
    items.push(item("Block", gfx::RED, Action::Relay("block", Some("uuid"), Some(uuid.into()))));
    menu(c, x, y, items);
}

fn send_composer(c: &mut Core) {
    let text = field_value(c, "composer").trim().to_owned();
    let Some(conv) = c.ui.conv.clone() else { return };
    if text.is_empty() {
        return;
    }
    let reply = c.ui.reply_to.clone();
    if c.send_message(&conv, &text, reply, None) {
        set_field(c, "composer", "");
        c.ui.reply_to = None;
        c.ui.chat_scroll = 0;
    }
}

fn send_friend_request(c: &mut Core) {
    let name = field_value(c, "addFriend").trim().to_owned();
    if !name.is_empty() {
        c.send("friend_request", Some("name"), Some(&name));
        set_field(c, "addFriend", "");
    }
}

fn create_group(c: &mut Core) {
    let name = field_value(c, "groupName").trim().to_owned();
    if !name.is_empty() {
        let members = std::mem::take(&mut c.ui.new_group_members);
        c.create_group(&name, members);
        set_field(c, "groupName", "");
    }
}

fn save_relay(c: &mut Core) {
    let url = field_value(c, "relayUrl").trim().to_owned();
    if !url.is_empty() {
        c.cfg.server_url = url;
    }
    c.cfg.save();
    c.reconnect();
    let u = c.cfg.server_url.clone();
    c.notice(&format!("Reconnecting to {u}"), false);
}

fn save_rename(c: &mut Core) {
    let name = field_value(c, "rename").trim().to_owned();
    if let Some(g) = c.ui.modal_subject.clone().and_then(|s| c.group(&s).cloned()) {
        if !name.is_empty() {
            c.group_action("group_rename", &g.id, Some("name"), Some(&name));
        }
    }
    c.ui.modal = None;
}

pub fn run(c: &mut Core, a: Action) {
    let [mx, my] = c.ui.mouse;
    match a {
        Action::Go(p) => go(c, p),
        Action::OpenChat(conv) => open_chat(c, &conv),
        Action::ConvMenu(conv) => conv_menu(c, &conv, mx, my + 10),
        Action::StatusMenu(x, y) => status_menu(c, x, y),
        Action::SetStatus(s) => c.set_status(&s),
        Action::ToggleMute => c.toggle_mute(),
        Action::ToggleDeafen => c.toggle_deafen(),
        Action::Relay(t, k, v) => c.send(t, k, v.as_deref()),
        Action::AcceptInvite(id) => c.accept_invite(&id),
        Action::DeclineInvite(id) => c.decline_invite(&id),
        Action::ReconnectVoice => c.reconnect_voice(),
        Action::FriendsTab(i) => {
            c.ui.friends_tab = i;
            c.ui.page_scroll = 0;
        }
        Action::SendFriendRequest => send_friend_request(c),
        Action::Kofi => {
            let _ = open::that_detached("https://ko-fi.com/annestinksbad");
        }
        Action::Call(uuid) => c.call(&uuid),
        Action::CallGroup(conv) => {
            if let Some(g) = c.group(&conv).cloned() {
                c.call_group(&g);
            }
        }
        Action::InviteToServer(conv) => {
            c.invite_to_server(&conv);
        }
        Action::OpenModal(m, s) => open_modal(c, m, s),
        Action::CloseModal => c.ui.modal = None,
        Action::ToggleMembers => c.ui.hide_members = !c.ui.hide_members,
        Action::SendComposer => send_composer(c),
        Action::SendSong(conv) => c.send_song(&conv),
        Action::ClearReply => c.ui.reply_to = None,
        Action::SetReply(r) => {
            c.ui.reply_to = Some(r);
            c.ui.focused = Some("composer".into());
        }
        Action::DeleteMessage(conv, id) => c.delete_message(&conv, &id),
        Action::LobbyUserMenu(uuid, name) => lobby_user_menu(c, &uuid, &name, mx, my),
        Action::MemberMenu(g, uuid) => member_menu(c, &g, &uuid, mx, my),
        Action::AddToGroupMenu(g) => add_to_group_menu(c, &g, mx, my - 80),
        Action::ToggleNewGroupMember(uuid) => {
            if let Some(i) = c.ui.new_group_members.iter().position(|u| *u == uuid) {
                c.ui.new_group_members.remove(i);
            } else {
                c.ui.new_group_members.push(uuid);
            }
        }
        Action::NewGroup => {
            c.ui.new_group_members.clear();
            go(c, Page::NewGroup);
        }
        Action::CreateGroup => create_group(c),
        Action::ToggleUserMute(uuid) => c.toggle_user_mute(&uuid),
        Action::GroupAction(t, g, k, v) => c.group_action(t, &g, k, v.as_deref()),
        Action::LobbyMute(uuid, m) => c.lobby_mute(&uuid, m),
        Action::SaveRelay => save_relay(c),
        Action::SetToggle(t) => {
            match t {
                Toggle::AutoConnect => c.cfg.auto_connect = !c.cfg.auto_connect,
                Toggle::Notifications => c.cfg.notifications = !c.cfg.notifications,
                Toggle::MessageSound => {
                    c.cfg.message_sound = !c.cfg.message_sound;
                    if c.cfg.message_sound {
                        c.fx.push(Effect::Ding);
                    }
                }
                Toggle::ShareListening => c.cfg.share_listening = !c.cfg.share_listening,
                Toggle::NoiseSuppression => c.cfg.noise_suppression = !c.cfg.noise_suppression,
                Toggle::EchoCancellation => c.cfg.echo_cancellation = !c.cfg.echo_cancellation,
                Toggle::AutoMutePaused => c.cfg.auto_mute_paused = !c.cfg.auto_mute_paused,
                Toggle::AutoMuteUnfocused => c.cfg.auto_mute_unfocused = !c.cfg.auto_mute_unfocused,
                Toggle::AutoRejoin => c.cfg.auto_rejoin = !c.cfg.auto_rejoin,
                Toggle::ShareServer => c.cfg.share_server = !c.cfg.share_server,
                Toggle::HudCompact => c.cfg.hud_compact = !c.cfg.hud_compact,
                Toggle::HudUnread => c.cfg.hud_unread = !c.cfg.hud_unread,
                Toggle::DebugLog => {
                    c.cfg.debug_log = !c.cfg.debug_log;
                    crate::dlog::set_enabled(c.cfg.debug_log);
                }
            }
            c.cfg.save();
            c.push_voice_params();
        }
        Action::SetPushToTalk(on) => {
            c.cfg.push_to_talk = on;
            c.cfg.save();
            c.push_voice_params();
        }
        Action::Slide(which) => {
            let (cx, track) = slider_geometry(c);
            c.ui.dragging = Some((which, cx, track));
            set_slider(c, which, mx);
        }
        Action::MicTest => {
            let on = !crate::voice().mic_test();
            c.push_voice_params();
            crate::voice().set_mic_test(on);
        }
        Action::CycleRingtone => {
            c.cfg.ringtone = if c.cfg.ringtone == "none" { "minecraft:block.note_block.bell".into() } else { "none".into() };
            if c.cfg.ringtone == "none" && c.ui.previewing {
                c.ui.previewing = false;
                c.fx.push(Effect::RingStop);
            }
            c.cfg.save();
        }
        Action::PreviewRingtone => {
            if !c.ui.previewing && c.cfg.ringtone != "none" {
                c.ui.previewing = true;
                c.fx.push(Effect::RingStart);
            } else {
                c.ui.previewing = false;
                if !c.ringing() {
                    c.fx.push(Effect::RingStop);
                }
            }
        }
        Action::ExportData => c.send("export_data", None, None),
        Action::PendingJoin(inv, from) => {
            c.ui.pending_join = Some(inv);
            open_modal(c, Modal::JoinServer, from);
        }
        Action::Join(inv) => {
            c.ui.modal = None;
            c.fx.push(Effect::CloseScreen);
            c.fx.push(Effect::Connect { name: inv.name, address: inv.address });
        }
        Action::DeleteAccount => {
            c.ui.modal = None;
            c.send("delete_account", Some("confirm"), Some("DELETE"));
        }
        Action::TrustNewKey(uuid) => {
            c.trust_new_key(&uuid);
            let name = c.conv_name(&uuid);
            c.notice(&format!("Now trusting {name}'s new key."), false);
            c.ui.modal = None;
        }
        Action::SaveRename => save_rename(c),
        Action::BlockedTab => {
            c.ui.friends_tab = 3;
            go(c, Page::Friends);
        }
        Action::Focus(id) => c.ui.focused = Some(id),
        Action::Addon(a) => addons::run(c, a),
        Action::OpenAddonPage(id, arg) => open_addon_page(c, &id, arg),
        Action::AskToJoin(uuid) => c.ask_to_join(&uuid),
        Action::AcceptRequest(from, kind) => c.accept_request(&from, &kind),
        Action::DismissRequest(from, kind) => c.dismiss_request(&from, &kind),
        Action::TogglePin(conv, id) => {
            let m = c.st.conversations.get(&conv).and_then(|l| l.iter().find(|m| m.id.as_deref() == Some(id.as_str()))).cloned();
            if let Some(m) = m {
                c.toggle_pin(&conv, &m);
            }
        }
        Action::Unpin(conv, id) => c.unpin(&conv, &id),
        Action::TogglePins => c.ui.show_pins = !c.ui.show_pins,
        Action::ToggleGroupMute(g) => {
            if !c.cfg.muted_groups.remove(&g) {
                c.cfg.muted_groups.insert(g);
            }
            c.cfg.save();
            c.changed();
        }
        Action::CopyGroupCode(conv) => match c.group_code(&conv) {
            Some(code) => {
                let ok = arboard::Clipboard::new().and_then(|mut cb| cb.set_text(code)).is_ok();
                c.notice(if ok { t("Invite code copied. Friends paste it on the Friends page.") } else { t("Couldn't copy the code.") }, !ok);
            }
            None => c.notice(t("Connect to VCraft first."), true),
        },
        Action::JoinWithCode => join_with_code(c),
        Action::SaveNote => save_note(c),
        Action::CycleEffect(forward) => {
            let cur = c.cfg.voice_effect.clone();
            c.cfg.voice_effect = if forward { crate::effects::next(&cur) } else { crate::effects::prev(&cur) }.to_owned();
            c.cfg.save();
            c.push_voice_params();
        }
        Action::CycleLanguage(forward) => {
            c.cfg.language = crate::i18n::cycle(&c.cfg.language, forward).to_owned();
            c.cfg.save();
            let lang = if c.cfg.language == "game" { c.world.language.clone() } else { c.cfg.language.clone() };
            crate::i18n::set(&lang);
        }
        Action::OpenLogFolder => {
            if let Some(dir) = crate::dlog::path().and_then(|p| p.parent().map(std::path::Path::to_path_buf)) {
                let _ = std::fs::create_dir_all(&dir);
                let _ = open::that_detached(dir);
            }
        }
        Action::None => {}
    }
}

fn join_with_code(c: &mut Core) {
    let code = field_value(c, "groupCode").trim().to_owned();
    if !code.is_empty() {
        c.join_with_code(&code);
        set_field(c, "groupCode", "");
    }
}

fn save_note(c: &mut Core) {
    if let Some(uuid) = c.ui.modal_subject.clone() {
        let (nick, note) = (field_value(c, "nick"), field_value(c, "note"));
        c.set_nickname(&uuid, &nick, &note);
    }
    c.ui.modal = None;
}

pub fn click(c: &mut Core, mouse: [f32; 2], button: i32) {
    c.ui.mouse = [mouse[0] as i32, mouse[1] as i32];
    let [mx, my] = c.ui.mouse;
    let inside = |r: [i32; 4]| mx >= r[0] && mx < r[0] + r[2] && my >= r[1] && my < r[1] + r[3];
    if c.ui.menu.is_some() {
        let hit = c.ui.overlay_hits.iter().rev().find(|h| inside(h.rect)).cloned();
        c.ui.menu = None;
        if let Some(h) = hit {
            run(c, h.left);
        }
        return;
    }
    let right = button == 1;
    let hit = c.ui.hits.iter().rev().find(|h| inside(h.rect)).cloned();
    match hit {
        Some(h) => {
            if !matches!(h.left, Action::Focus(_)) {
                c.ui.focused = None;
            }
            let a = if right { h.right.unwrap_or(h.left) } else { h.left };
            run(c, a);
        }
        None => c.ui.focused = None,
    }
}

pub fn release(c: &mut Core) {
    if c.ui.dragging.take().is_some() {
        c.cfg.save();
    }
}

pub fn drag(c: &mut Core, mouse: [f32; 2]) {
    c.ui.mouse = [mouse[0] as i32, mouse[1] as i32];
    if let Some((which, _, _)) = c.ui.dragging {
        set_slider(c, which, mouse[0] as i32);
    }
}

pub fn scroll(c: &mut Core, mouse: [f32; 2], amount: f64) {
    if c.ui.menu.is_some() {
        return;
    }
    let step = (amount * 24.0).round() as i32;
    let (mx, my) = (mouse[0] as i32, mouse[1] as i32);
    let inside = |r: [i32; 4]| mx >= r[0] && mx < r[0] + r[2] && my >= r[1] && my < r[1] + r[3];
    if inside(c.ui.sb) {
        c.ui.sidebar_scroll = (c.ui.sidebar_scroll - step).clamp(0, c.ui.sidebar_max.max(0));
    } else if c.ui.page == Page::Chat && inside(c.ui.chat) {
        c.ui.chat_scroll = (c.ui.chat_scroll + step).clamp(0, c.ui.chat_max.max(0));
    } else {
        c.ui.page_scroll = (c.ui.page_scroll - step).clamp(0, c.ui.page_max.max(0));
    }
}

fn enter(c: &mut Core) {
    match c.ui.focused.as_deref() {
        Some("composer") => send_composer(c),
        Some("addFriend") => send_friend_request(c),
        Some("relayUrl") => save_relay(c),
        Some("groupName") => create_group(c),
        Some("rename") => save_rename(c),
        Some("nick" | "note") => save_note(c),
        Some("groupCode") => join_with_code(c),
        Some(id) if id.contains('.') => addons::field_enter(c, &id.to_owned()),
        _ => {}
    }
}

const SHIFT: u32 = 1;
const CONTROL: u32 = 2;
const SUPER: u32 = 8;

pub fn key(c: &mut Core, key: &str, modifiers: u32, open_key: &str) {
    let ctrl = modifiers & (CONTROL | SUPER) != 0;
    match key {
        "key.keyboard.escape" => {
            if c.ui.menu.is_some() {
                c.ui.menu = None;
            } else if c.ui.modal.is_some() {
                c.ui.modal = None;
            } else if c.ui.focused.is_some() {
                c.ui.focused = None;
            } else if c.ui.reply_to.is_some() && c.ui.page == Page::Chat {
                c.ui.reply_to = None;
            } else {
                c.fx.push(Effect::CloseScreen);
            }
        }
        "key.keyboard.enter" | "key.keyboard.keypad.enter" => enter(c),
        "key.keyboard.backspace" => {
            if let Some(id) = c.ui.focused.clone() {
                let f = c.ui.fields.entry(id).or_default();
                if ctrl {
                    let t = f.value.trim_end().to_owned();
                    let cut = t.rfind(' ').map_or(0, |i| i + 1);
                    f.value.truncate(cut);
                } else {
                    f.value.pop();
                }
            }
        }
        "key.keyboard.v" if ctrl => {
            if let Some(id) = c.ui.focused.clone() {
                if let Ok(text) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
                    let f = c.ui.fields.entry(id).or_default();
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        if f.value.chars().count() >= f.max.max(1) {
                            break;
                        }
                        f.value.push(ch);
                    }
                }
            }
        }
        "key.keyboard.c" if ctrl => {
            if let Some(id) = c.ui.focused.clone() {
                let v = c.ui.fields.get(&id).map(|f| f.value.clone()).unwrap_or_default();
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(v);
                }
            }
        }
        k if k == open_key && c.ui.focused.is_none() && c.ui.modal.is_none() => c.fx.push(Effect::CloseScreen),
        _ => {}
    }
    let _ = SHIFT;
}

pub fn typed(c: &mut Core, ch: char) {
    if ch.is_control() || ch == '\u{a7}' {
        return;
    }
    if c.ui.focused.is_none() && c.ui.page == Page::Chat && c.ui.modal.is_none() {
        c.ui.focused = Some("composer".into());
    }
    if let Some(id) = c.ui.focused.clone() {
        let f = c.ui.fields.entry(id).or_insert_with(|| Field { value: String::new(), max: 256 });
        if f.value.chars().count() < f.max.max(1) {
            f.value.push(ch);
        }
    }
}

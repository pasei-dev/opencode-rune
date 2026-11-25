//! `App`-independent contract `ui.*` host node builders over `opencode-ui` and `rune-kit`:
//! `frame`, `divider`, `icon`, `loader`, `metric`, `keybind`, `button`, `iconButton`, `radio`, `checkbox`,
//! `serverIndicator`, `dialogHeader`, `progressCircle`, and `segmented`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use opencode_plugin_spec::NodeKind;
use opencode_ui::palette::{Pal, kit, pal, token};
use opencode_ui::{keymap, odometer, widgets};
use rune::{Chord, Color, Direction, El, Icon, Shadow, Spring, TextStyle, col, el, icon as rune_icon, row, text};
use rune_kit::{ButtonVariant, Loader};
use rune_luau::{Data, HostNode, NodeArgs};

use crate::icons;

thread_local! {
    static PLUGIN: RefCell<Option<Rc<str>>> = const { RefCell::new(None) };
}

/// Runs `f` as plugin `plugin`'s render: nodes that act for their plugin (`ui.icon`, `ui.flyoutRow`,
/// `ui.image`, `ui.video`) read it with [`rendering`].
pub fn rendering_as<R>(plugin: &Rc<str>, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Rc<str>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            PLUGIN.set(self.0.take());
        }
    }
    let _restore = Restore(PLUGIN.replace(Some(plugin.clone())));
    f()
}

/// The plugin whose render runs now.
pub fn rendering() -> Option<Rc<str>> {
    PLUGIN.with_borrow(Clone::clone)
}

/// Icon `name`: the rendering plugin's own (`icons/<name>.svg`), else the host's or the app's.
pub fn named_icon(name: &str) -> &'static Icon {
    icons::resolve(rendering().as_deref().unwrap_or_default(), name)
}

/// The `App`-independent contract UI nodes. `command_chord` resolves a command id's active
/// shortcut (for `ui.iconButton`'s tooltip); `catalog_chord` resolves a command's default chord
/// from the host catalog when no user override exists (for `ui.keybind`).
pub fn base_nodes<S: 'static>(
    command_chord: fn(&str) -> Option<Chord>,
    catalog_chord: fn(&str) -> Option<Chord>,
) -> Vec<HostNode> {
    vec![
        node("frame", frame::<S>),
        node("divider", divider),
        node("icon", icon),
        node("metric", metric),
        node("keybind", move |a| keybind(a, catalog_chord)),
        node("button", button),
        node("iconButton", move |a| icon_button(a, command_chord)),
        node("radio", radio),
        node("checkbox", checkbox),
        node("loader", |a| Ok(Loader::new().size(a.number("size").unwrap_or(16.0) as f32).build(kit(&p(a))))),
        node("serverIndicator", server_indicator),
        node("progressCircle", progress_circle),
        node("segmented", segmented),
    ]
}

/// Contract node `name`, built by `build`, with the contract's props for it listed
/// (`HostNode::props`): a prop a script passes that is neither one of them, nor read by `build`,
/// nor a standard prop is an error in debug builds and a warning in release, instead of being
/// dropped without a word.
pub fn node(name: &'static str, build: impl Fn(&mut NodeArgs) -> Result<El, String> + 'static) -> HostNode {
    let host = HostNode::new(name, build);
    match NodeKind::parse(name) {
        Some(kind) => host.props(move |key| kind.props().get(key).is_some()),
        None => host,
    }
}

fn p(a: &NodeArgs) -> Pal {
    *pal(a.dark())
}

/// The text style the props describe, from `base`.
pub fn text_style(a: &NodeArgs, base: TextStyle) -> TextStyle {
    let mut s = base;
    if let Some(n) = a.number("size") {
        s.size = n as f32;
    }
    s.weight = match a.string("weight") {
        Some("regular") => 440.0,
        Some("medium") => 530.0,
        Some("semibold") => 600.0,
        Some("bold") => 700.0,
        _ => a.number("weight").map_or(s.weight, |w| w as f32),
    };
    if let Some(n) = a.number("lineHeight") {
        s = s.lh(n as f32);
    }
    if let Some(n) = a.number("tracking") {
        s = s.tracking(n as f32);
    }
    s
}

/// HTML's `dir` on a text input (`dir="auto"` on Desktop's name fields).
pub fn dir(a: &NodeArgs) -> Option<Direction> {
    match a.string("dir")? {
        "ltr" => Some(Direction::Ltr),
        "rtl" => Some(Direction::Rtl),
        "auto" => Some(Direction::Auto),
        _ => None,
    }
}

/// A layout container whose border takes room in layout (`El::border_layout`), snapped to whole
/// device pixels at every scale the way CSS `box-sizing: border-box` does. Standard `UiBoxProps`
/// apply automatically via `rune-luau`'s host-node prop pass.
fn frame<S: 'static>(a: &mut NodeArgs) -> Result<El, String> {
    let mut out = match a.string("direction") {
        Some("row") => row(),
        Some("box") => el(),
        Some("col") | None => col(),
        Some(other) => return Err(format!("direction must be col, row or box, got {other:?}")),
    };
    out = out.border_layout();
    if let Some(Data::List(sides)) = a.data("borderSides") {
        let _ = a.get("borderWidth");
        let w = |i: usize| match sides.get(i) {
            Some(Data::Number(n)) => *n as f32,
            Some(Data::Integer(n)) => *n as f32,
            _ => 0.0,
        };
        let c = a.color("border").unwrap_or_else(|| p(a).border);
        out = out.border_sides([w(0), w(1), w(2), w(3)], c);
    }
    out = out.children(std::mem::take(&mut a.children));
    if a.flag("disabled") {
        out = out.disabled(true);
    } else {
        out = a.on_press(out, "onPress");
    }
    out = a.on_hover(out, "onHover");
    if a.flag("focusGroup") {
        out = out.focus_group();
    }
    out = key_handler::<S>(a, out);
    Ok(focus(a, out))
}

/// `autofocus` (`El::autofocus`: the control a dialog focuses as it opens) and `focusReturn`
/// (`El::focus_return`: where focus goes when this leaves while it has focus).
fn focus(a: &NodeArgs, el: El) -> El {
    let el = if a.flag("autofocus") { el.autofocus() } else { el };
    match a.string("focusReturn") {
        Some(key) => el.focus_return(key),
        None => el,
    }
}

/// `keys` and `onKey`: a key `keys` names, pressed with no modifier while focus is on the frame or
/// inside it (not in a text field, whose keys stay its own), runs `onKey` with its name in
/// `event.keyName` instead of doing what it does there: Return in a list that imports rather than
/// ticks the focused row.
fn key_handler<S: 'static>(a: &NodeArgs, el: El) -> El {
    let Some(press) = a.press_later("onKey") else { return el };
    let Some(Data::List(names)) = a.data("keys") else { return el };
    let keys: Vec<(rune::Key, &'static str)> = names
        .iter()
        .filter_map(|name| match name.as_str()? {
            "Enter" => Some((rune::Key::Enter, "Enter")),
            "Escape" => Some((rune::Key::Escape, "Escape")),
            "ArrowUp" => Some((rune::Key::Up, "ArrowUp")),
            "ArrowDown" => Some((rune::Key::Down, "ArrowDown")),
            "ArrowLeft" => Some((rune::Key::Left, "ArrowLeft")),
            "ArrowRight" => Some((rune::Key::Right, "ArrowRight")),
            "Home" => Some((rune::Key::Home, "Home")),
            "End" => Some((rune::Key::End, "End")),
            "PageUp" => Some((rune::Key::PageUp, "PageUp")),
            "PageDown" => Some((rune::Key::PageDown, "PageDown")),
            "Delete" => Some((rune::Key::Delete, "Delete")),
            "Backspace" => Some((rune::Key::Backspace, "Backspace")),
            _ => None,
        })
        .collect();
    if keys.is_empty() {
        return el;
    }
    el.on_key(move |s: &mut S, ev: &rune::KeyEvent, tx: &mut rune::Tx| {
        if ev.mods != rune::Mods::default() {
            return false;
        }
        let Some(&(_, name)) = keys.iter().find(|(key, _)| ev.key == *key) else { return false };
        press.run(s, tx, vec![("keyName", Data::String(name.to_owned()))]);
        true
    })
}

/// A 1 px rule laid out as Chromium lays out a 1px CSS border: one device pixel at 1x, 1.25x and
/// 1.5x, two at 2x (`rune::rule`), so a box that holds one ends where Desktop's does.
fn divider(a: &mut NodeArgs) -> Result<El, String> {
    let c = a.color("color").unwrap_or(p(a).border_muted);
    Ok(if a.flag("vertical") { el().h_full().border_sides([0.0, 0.0, 0.0, 1.0], c).border_layout() } else { rune::rule(1.0, c).w_full() })
}

fn icon(a: &mut NodeArgs) -> Result<El, String> {
    let name = a.content().unwrap_or_default().to_owned();
    let size = a.number("size").unwrap_or(16.0) as f32;
    let pal = p(a);
    let color = a.color("color").unwrap_or(match a.string("tone") {
        Some("success") => pal.success,
        Some("warning") => pal.warning,
        Some("danger") => pal.danger,
        Some("accent") => pal.v2.icon_accent,
        _ => pal.icon,
    });
    Ok(rune_icon(named_icon(&name), size).color(color))
}

/// A number whose changed digits roll (`opencode_ui::odometer`), or plain text.
fn metric(a: &mut NodeArgs) -> Result<El, String> {
    let value = a.number("value").unwrap_or(0.0);
    let words = a.string("text").unwrap_or_default().to_owned();
    let mut font = text_style(a, TextStyle::new(13.0, 440.0));
    if a.get("tabular").is_none() || a.flag("tabular") {
        font = font.features("\"tnum\"");
    }
    let color = a.color("color").unwrap_or(p(a).text);
    let animate = a.get("animate").is_none() || a.flag("animate");
    if !animate {
        return Ok(text(words).font(font).color(color));
    }
    let key = a.string("key").unwrap_or("metric").to_owned();
    Ok(odometer::odometer_end(format!("luau:{key}"), words, value, font, color))
}

fn keybind(a: &mut NodeArgs, catalog_chord: fn(&str) -> Option<Chord>) -> Result<El, String> {
    let caps = match (a.string("keys"), a.string("command")) {
        (Some(keys), _) => keys
            .parse::<rune::Keybind>()
            .ok()
            .and_then(|k| k.chords().first().copied())
            .map(keymap::caps)
            .unwrap_or_else(|| vec![keys.to_owned()]),
        // A chord Settings bound the command shows one keycap per stroke (opencode#64395).
        (None, Some(command)) => match keymap::sequence::first(command) {
            Some(strokes) => keymap::sequence::caps(&strokes),
            None => keymap::overrides::command(command)
                .and_then(|k| k.chords().first().copied())
                .or_else(|| catalog_chord(command))
                .map(keymap::caps)
                .unwrap_or_default(),
        },
        _ => Vec::new(),
    };
    Ok(keymap::view::chips(&p(a), caps))
}

fn variant(name: Option<&str>) -> ButtonVariant {
    match name {
        Some("contrast") => ButtonVariant::Contrast,
        Some("primary") => ButtonVariant::Submit,
        Some("ghost") => ButtonVariant::Ghost,
        Some("ghost-muted") => ButtonVariant::GhostMuted,
        Some("outline") => ButtonVariant::Outline,
        Some("danger") => ButtonVariant::Danger,
        _ => ButtonVariant::Neutral,
    }
}

#[allow(deprecated)]
fn button(a: &mut NodeArgs) -> Result<El, String> {
    let pal = p(a);
    let k = kit(&pal);
    let words = a.string("text").unwrap_or_default().to_owned();
    let (disabled, loading) = (a.flag("disabled"), a.flag("loading"));
    let small = a.string("size") == Some("small");
    // Desktop's button sizes (`button.css`): small 24 tall, normal 28, large 32 with 15 a side.
    #[allow(deprecated)]
    let size = match a.string("size") {
        Some("small") => rune_kit::ButtonSize::Small,
        Some("large") => rune_kit::ButtonSize::Large,
        _ => rune_kit::ButtonSize::Normal,
    };
    let v = if loading && !small { ButtonVariant::Loading } else { variant(a.string("variant")) };
    let mut b = widgets::button(words.clone(), v).size(size).disabled(disabled);
    if let Some(lead) = a.string("icon") {
        b = b.icon(named_icon(lead));
    }
    if loading && small {
        // `<Loader class="size-3.5" />` before the label, in the label's color.
        let spinner = Loader::new().size(14.0).build(k);
        b = b.label_with(move |style, ink| row().gap(4.0).center_y().child(spinner).child(text(words).font(style).color(ink)));
    } else if loading {
        // Desktop's `variant="loading"` button holds only `<Loader />`.
        let spinner = Loader::new().size(16.0).build(k);
        b = b.label_with(move |_, _| spinner);
    }
    let el = focus(a, b.build(k));
    Ok(if disabled || loading { el } else { a.on_press(el, "onPress") })
}

fn icon_button(a: &mut NodeArgs, command_chord: fn(&str) -> Option<Chord>) -> Result<El, String> {
    let pal = p(a);
    let name = a.string("icon").unwrap_or_default().to_owned();
    let small = a.string("size") == Some("small");
    let side = if small { 24.0 } else { 28.0 };
    let pressed = a.flag("pressed");
    let mut b = row().role("button").size(side, side).center().radius(6.0).hover(pal.hover).press(pal.pressed).hover_motion(rune::Anim::INSTANT, rune::Anim::INSTANT);
    if pressed {
        b = b.fill(pal.pressed);
    }
    // ghost-muted draws its icon in icon-muted, icon-base while pressed (`icon-button.css`).
    let ink = a.color("color").unwrap_or(if a.string("variant") == Some("ghost-muted") && !pressed { pal.icon_muted } else { pal.icon });
    let mut b = b.child(rune_icon(named_icon(&name), 16.0).color(ink));
    if let Some(command) = a.string("keybind") {
        // A chord Settings bound the command shows one keycap per stroke (opencode#64395).
        let caps = match keymap::sequence::first(command) {
            Some(strokes) => keymap::sequence::caps(&strokes),
            None => command_chord(command).map_or_else(Vec::new, keymap::caps),
        };
        if !caps.is_empty() {
            b = b.help_keys(caps);
        }
    }
    let b = focus(a, b);
    Ok(if a.flag("disabled") { b.layer_opacity(0.5) } else { a.on_press(b, "onPress") })
}

fn radio(a: &mut NodeArgs) -> Result<El, String> {
    let pal = p(a);
    let checked = a.flag("checked");
    // A description's tone is Desktop's `state-fg-*` (`data-tone` on a status line).
    let tone = match a.string("tone") {
        Some(t @ ("success" | "warning" | "danger")) => token(pal.dark, &format!("state-fg-{t}")),
        Some("accent") => token(pal.dark, "text-text-accent"),
        _ => None,
    };
    let r = radio_widget(kit(&pal), a.string("text").unwrap_or_default(), a.string("description").unwrap_or_default(), checked, a.flag("disabled"), tone, a.number("lineHeight").unwrap_or(20.0) as f32);
    Ok(if a.flag("disabled") { r } else { a.on_press_with(r, "onChange", vec![("checked", Data::Bool(true))]) })
}

fn checkbox(a: &mut NodeArgs) -> Result<El, String> {
    let pal = p(a);
    let checked = a.flag("checked");
    let disabled = a.flag("disabled");
    let layer = a.string("surface") == Some("layer-1");
    let c = checkbox_widget(kit(&pal), a.string("text").unwrap_or_default(), a.string("description").unwrap_or_default(), checked, disabled, layer);
    Ok(if disabled { c } else { a.on_press_with(c, "onChange", vec![("checked", Data::Bool(!checked))]) })
}

/// A `checkbox-v2` (`checkbox-compact.css`, Desktop's `Checkbox appearance="compact"`): the 16 pt
/// box 2 down in its 16 × 20 stack, radius 4, on `bg-base` (`layer-1` on that surface) with a
/// 0.5 hairline, ticked the accent with its 20% white sheen and the 1 pt check; the 13/440 label on
/// a 20 line, 8 after it, and the 11/440 description 6 under it. Disabled, the row is half opacity.
fn checkbox_widget(k: &rune_kit::Kit, label: &str, description: &str, checked: bool, disabled: bool, layer: bool) -> El {
    let c = &k.c;
    let base = if layer { c.layer_01 } else { c.bg_base };
    let hairline = Shadow { spread: 0.5, color: c.border_strong, inset: true, ..Shadow::default() };
    let mut control = el().key("control").size(16.0, 16.0).absolute(0.0, 2.0).center().radius(4.0).box_shadow(hairline);
    control = if checked {
        control.fill(c.bg_accent).gradient(k.light_on(0.2, c.bg_accent), k.light(0.0))
    } else {
        control.fill(base)
    };
    if !disabled {
        control = control.hover(if checked { over(k.on(c.contrast_hover, c.bg_accent), c.bg_accent) } else { over(c.hover, base) });
    }
    control = control.child(rune_icon(&rune_theme_opencode::icons::CHECK, 16.0).key("check").color(c.icon_contrast).opacity(if checked { 1.0 } else { 0.0 }));
    let mut words = col().key("text").gap(6.0).grow(1.0).shrink().min_w(0.0).child(text(label.to_owned()).key("label").font(TextStyle::new(13.0, 440.0).lh(20.0).tracking(-0.04)).color(c.text_base));
    if !description.is_empty() {
        words = words.child(text(description.to_owned()).key("description").font(TextStyle::new(11.0, 440.0).lh(16.0).tracking(0.05)).color(c.text_muted));
    }
    let r = row()
        .role("checkbox")
        .label(label.to_owned())
        .checked(checked)
        .gap(8.0)
        .items_start()
        .disabled(disabled)
        .child(el().key("stack").size(16.0, 20.0).child(control).a11y_hidden())
        .child(words.a11y_hidden());
    if disabled { r.layer_opacity(0.5) } else { r }
}

/// `top` over `bottom`, as CSS layers paint.
fn over(top: Color, bottom: Color) -> Color {
    let a = top.a + bottom.a * (1.0 - top.a);
    if a <= 0.0 {
        return Color::TRANSPARENT;
    }
    let mix = |t: f32, b: f32| (t * top.a + b * bottom.a * (1.0 - top.a)) / a;
    Color { r: mix(top.r, bottom.r), g: mix(top.g, bottom.g), b: mix(top.b, bottom.b), a }
}

/// A `radio-v2` item (`radio.css`): the 16 pt control in its 16 × 20 stack, the 13/440 label on
/// a 20 line and the 12/440 description under it, 8 apart; the description in `tone` when given.
/// A control that is not selected is flat on `bg-base`; only the selected one has the top light
/// (`packages/ui/src/forms/radio/radio.css:92`, `:125`). The dot has the depth gradient over its
/// grey, a ring in the contrast icon colour and the small elements' drop (`radio.css:140-160`).
fn radio_widget(k: &rune_kit::Kit, label: &str, description: &str, checked: bool, disabled: bool, tone: Option<Color>, label_lh: f32) -> El {
    let c = &k.c;
    let mut control = el()
        .key("control")
        .size(16.0, 16.0)
        .absolute(0.0, 2.0)
        .radius(8.0)
        .box_shadow(Shadow { spread: 0.5, color: c.border_strong, inset: true, ..Shadow::default() });
    control = if checked {
        control.fill(c.bg_accent).gradient(k.light_on(0.2, c.bg_accent), k.light(0.0))
    } else {
        control.fill(c.bg_base)
    };
    if checked {
        let dot = el()
            .key("dot")
            .size(6.0, 6.0)
            .absolute(5.0, 5.0)
            .radius(3.0)
            .fill(c.knob_on)
            .gradient(k.on(c.depth_top, c.knob_on), k.on(c.depth_bottom, c.knob_on))
            .box_shadow(Shadow { spread: 0.5, color: c.icon_contrast, inset: true, ..Shadow::default() });
        control = control.child(k.elevate(dot, &k.elevation.elements));
        if !disabled {
            control = control.hover(over(k.on(c.contrast_hover, c.bg_accent), c.bg_accent));
        }
    } else if !disabled {
        control = control.hover(over(c.hover, c.bg_base));
    }
    if disabled {
        control = control.layer_opacity(0.5);
    }
    let ink = if disabled { c.text_muted } else { c.text_base };
    // The 16-point text sits centred in its line box.
    let lead = ((label_lh - 16.0) / 2.0).max(0.0);
    let mut words = col().key("text").gap(6.0 + lead).grow(1.0).shrink().min_w(0.0).child(text(label.to_owned()).key("label").font(TextStyle::new(13.0, 440.0).lh(16.0).tracking(-0.04)).color(ink).mt(lead));
    if !description.is_empty() {
        let ink = if disabled { c.text_faint } else { tone.unwrap_or(c.text_muted) };
        words = words.child(text(description.to_owned()).key("description").font(TextStyle::new(12.0, 440.0).lh(12.0).tracking(0.05).features("\"tnum\"")).color(ink));
    }
    row()
        .role("radio")
        .label(label.to_owned())
        .gap(8.0)
        .items_start()
        .focus_selected(checked)
        .disabled(disabled)
        .child(el().key("stack").size(16.0, 20.0).child(control).a11y_hidden())
        .child(words.a11y_hidden())
}

/// Desktop's `ServerHealthIndicator` (`servers/registry/row.tsx`) as a plugin row's
/// `ServerRow.Indicator`: a lock while it waits for sign-in, a spinner while it connects, a
/// warning when incompatible, else a 6-point dot (green once healthy, red when unreachable,
/// faint when unknown).
fn server_indicator(a: &mut NodeArgs) -> Result<El, String> {
    let pal = p(a);
    let health = a.data("health");
    let field = |k: &str| health.as_ref().map(|h| h.get(k).clone()).unwrap_or(Data::Nil);
    let slot = || row().w(6.0).h(14.0).center();
    if a.flag("auth") {
        return Ok(slot().role("status").child(rune_icon(named_icon("lock"), 12.0).color(pal.icon_muted)));
    }
    if a.flag("connecting") || field("checking") == Data::Bool(true) {
        return Ok(slot().role("status").child(Loader::new().size(12.0).build(kit(&pal))));
    }
    if field("incompatible") == Data::Bool(true) {
        return Ok(rune_icon(named_icon("warning"), 16.0).color(pal.v1_icon_warning));
    }
    // Desktop's v1 status colours, as the host's own server rows draw them.
    let color = match field("healthy") {
        Data::Bool(true) => widgets::status_ink(&pal, "connected"),
        Data::Bool(false) => widgets::status_ink(&pal, "failed"),
        _ => pal.icon_faint,
    };
    Ok(el().size(6.0, 6.0).my(3.5).radius(3.0).fill(color))
}

/// `ui.dialogHeader` (`dialog.css`): `dialog-header` 16 in, its `dialog-title-group`, and the
/// 20 pt close button wired by `attach_close`.
pub fn dialog_header(a: &mut NodeArgs, attach_close: impl FnOnce(El) -> El) -> Result<El, String> {
    let pal = p(a);
    let title = a.string("title").unwrap_or_default().to_owned();
    let mut title_el = text(title).font(TextStyle::new(15.0, 530.0).lh(20.0).tracking(-0.13)).color(pal.text);
    if a.get("wrapTitle").is_none() || a.flag("wrapTitle") {
        title_el = title_el.wrap_anywhere().w_full();
    }
    let mut group = col()
        .key("dialog-title-group")
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(4.0)
        .child(title_el);
    if let Some(description) = a.string("description") {
        group = group.child(text(description.to_owned()).font(widgets::BODY).color(pal.muted).wrap_anywhere().w_full());
    }
    let mut header = row().key("dialog-header").w_full().padding(16.0).child(group);
    if !a.flag("hideClose") {
        let label = a.string("closeLabel").map_or_else(|| opencode_i18n::t!("common.close").to_owned(), str::to_owned);
        let close = el()
            .key("dialog-close-button")
            .role("button")
            .label(label)
            .size(20.0, 20.0)
            .mt(-2.0)
            .mr(-2.0)
            .radius(4.0)
            .center()
            .hover(pal.hover)
            .press(pal.pressed)
            .hover_motion(rune::Anim::INSTANT, rune::Anim::INSTANT)
            .child(rune_icon(&widgets::DIALOG_CLOSE, 16.0).color(pal.icon_muted));
        header = header.child(attach_close(close));
    }
    Ok(header)
}

/// The app's `ProgressCircle`: a track and the arc from twelve o'clock, clockwise; `compact` a
/// 14-unit ring with a 1.5 stroke, `indicator` a 16-unit one with an overlay over its track.
fn progress_circle(a: &mut NodeArgs) -> Result<El, String> {
    let pal = p(a);
    let indicator = a.string("appearance") == Some("indicator");
    let view: f32 = if indicator { 16.0 } else { 14.0 };
    let stroke = a.number("strokeWidth").map_or(if indicator { 3.0 } else { 1.5 }, |s| s as f32);
    let size = a.number("size").map_or(view, |s| s as f32);
    let percent = a.number("percentage").unwrap_or(0.0).clamp(0.0, 100.0).round() as u32;
    let (track, arc) = ring_icons(view, stroke, percent);
    let icon = icons::icon;
    let place = |e: El| e.pin(Some(0.0), Some(0.0), None, None);
    let mut out = el().size(size, size);
    if let Some(track) = icon(track) {
        out = out.child(place(rune_icon(track, size).color(a.color("track").unwrap_or(pal.q.layer4))));
        if let (true, Some(overlay)) = (indicator, a.color("overlay")) {
            out = out.child(place(rune_icon(track, size).color(overlay)));
        }
    }
    if let Some(arc) = arc.and_then(icon) {
        out = out.child(place(rune_icon(arc, size).color(a.color("progress").unwrap_or(pal.icon))));
    }
    Ok(out)
}

/// The ring's track and arc icons (`None` for an empty arc), interned once per look and percent.
fn ring_icons(view: f32, stroke: f32, percent: u32) -> (u32, Option<u32>) {
    type RingCache = HashMap<(u32, u32, u32), (u32, Option<u32>)>;
    thread_local! {
        static RINGS: RefCell<RingCache> = RefCell::default();
    }
    let key = (view.to_bits(), stroke.to_bits(), percent);
    if let Some(found) = RINGS.with_borrow(|r| r.get(&key).copied()) {
        return found;
    }
    let (c, r) = (view / 2.0, view / 2.0 - stroke / 2.0);
    let circle = format!(r#"<circle cx="{c}" cy="{c}" r="{r}" stroke="currentColor" stroke-width="{stroke}"/>"#);
    let arc = match percent {
        0 => None,
        100 => Some(circle.clone()),
        _ => {
            let angle = percent as f32 / 100.0 * std::f32::consts::TAU;
            let (x, y) = (c + r * angle.sin(), c - r * angle.cos());
            Some(format!(r#"<path d="M{c} {top}A{r} {r} 0 {large} 1 {x:.3} {y:.3}" stroke="currentColor" stroke-width="{stroke}"/>"#, top = c - r, large = u8::from(percent > 50)))
        }
    };
    let track = icons::intern_icon("luau", &format!("ring-track-{view}-{stroke}"), &circle, view);
    let arc = arc.map(|a| icons::intern_icon("luau", &format!("ring-arc-{view}-{stroke}-{percent}"), &a, view));
    RINGS.with_borrow_mut(|r| r.insert(key, (track, arc)));
    (track, arc)
}

/// `ui.segmented`: the app's segmented control (`segmented-control-v2`): a 28 pt track on
/// `layer-01` under a 0.5 pt `border-base` ring, each item 40 × 28 with its 16 pt icon, the picked
/// one on `bg-base` with a 0.5 pt `border-strong` ring.
fn segmented(a: &mut NodeArgs) -> Result<El, String> {
    let pal = p(a);
    let value = a.string("value").unwrap_or_default().to_owned();
    let Some(Data::List(items)) = a.data("items") else { return Err("ui.segmented needs items".into()) };
    let mut track = row().h(28.0).center_y().radius(6.0).fill(pal.layer1).box_shadow(Pal::ring_out(pal.border));
    for item in items {
        let id = item.get("id").as_str().unwrap_or_default().to_owned();
        let label = item.get("label").as_str().unwrap_or_default().to_owned();
        let glyph = named_icon(item.get("icon").as_str().unwrap_or_default());
        let key = item.get("key").as_str().map_or_else(|| id.clone(), str::to_owned);
        let on = id == value;
        let b = segmented_button(&pal, key, glyph, on, label);
        track = track.child(if on { b } else { a.on_press_with(b, "onChange", vec![("value", Data::String(id))]) });
    }
    Ok(track)
}

fn segmented_button(p: &Pal, key: String, glyph: &'static Icon, active: bool, help: String) -> El {
    el().key(key)
        .size(40.0, 28.0)
        .radius(6.0)
        .center()
        .motion(Spring::NONE)
        .fill(if active { p.bg_base } else { Color::TRANSPARENT })
        .box_shadow(Pal::ring_out(if active { p.q.border_strong } else { Color::TRANSPARENT }))
        .role("button")
        .label(help.clone())
        .help(help)
        .child(rune_icon(glyph, 16.0).color(if active { p.icon } else { p.icon_muted }).group_color(p.icon))
        .hover_group()
}

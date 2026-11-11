//! The footer: help for the control under the pointer on the left, and
//! the background tasks (analysis, deck loads, imports, patch errors) in a
//! well on the right, with a tray listing them all when there are several.
//!
//! Help is recorded by the widgets as the frame is laid out, after the
//! footer's panel has been placed, so the footer keeps the help zone's
//! rect and paints its text at the end of the frame.

use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Id, LayerId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2,
    vec2,
};

use crate::key::{Key, Lit};
use crate::theme;

/// The footer panel's height.
pub const FOOTER_H: f32 = 37.0;
/// Panel padding, and the gap between the help zone and the task well.
const PAD: f32 = 4.0;
const ZONE_GAP: f32 = 6.0;
const ZONE_H: f32 = 29.0;
const HELP_PAD: f32 = 9.0;
/// The task well, and the least width the help zone keeps before the well
/// gives way.
const WELL_W: f32 = 288.0;
const HELP_MIN_W: f32 = 172.0;
const WELL_MIN_W: f32 = 115.0;
const WELL_PAD_LEFT: f32 = 9.0;
const WELL_PAD_RIGHT: f32 = 4.0;
const ITEM_GAP: f32 = 7.0;
const LED: f32 = 5.0;
const WARN_GLYPH: f32 = 13.0;
/// Attention: a `warn` wash at 8% and a 1px outline inside the edge.
const WARN_WASH: f32 = 0.08;
const WARN_OUTLINE: f32 = 1.0;
/// The progress strip: 1.5px, 9 in from each side, 2 up from the bottom.
const STRIP_H: f32 = 1.5;
const STRIP_INSET: f32 = 9.0;
const STRIP_BOTTOM: f32 = 2.0;
/// The `+N` key and its label.
const MORE_KEY: Vec2 = vec2(32.0, 22.0);
const MORE_LABEL: theme::TypeStyle = theme::NUMERALS_SM;
/// The tray: right-aligned above the footer.
const TRAY_W: f32 = 388.0;
const TRAY_INSET: f32 = 4.0;
const TRAY_GAP: f32 = 3.0;
const TRAY_PAD: f32 = 4.0;
const TRAY_ROW_H: f32 = 32.0;
/// How long a finished task stays.
pub const DONE_FOR: Duration = Duration::from_secs(4);

/// A task's state, in priority order: what needs attention first, then
/// what's under way, then what just finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskKind {
    Attention,
    Running,
    Done,
}

/// Which task: one per source, so an update replaces what it said before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskId {
    Analysis,
    Import,
    /// A deck's (or the Prepare player's) load, by its label.
    Load(&'static str),
    Patch,
    Audio,
    /// A one-off message, such as a confirmation.
    Note(u64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub id: TaskId,
    pub kind: TaskKind,
    pub message: String,
    /// 0..1 when it's known.
    pub progress: Option<f32>,
    /// When it started, as a sequence number: the order within a kind.
    started: u64,
    /// When it finished, for expiry.
    done_at: Option<Instant>,
}

/// The background tasks the footer shows.
#[derive(Default)]
pub struct Tasks {
    tasks: Vec<Task>,
    seq: u64,
}

impl Tasks {
    /// Start or update the task `id`. It keeps its place in the order it
    /// started, whatever its kind becomes.
    pub fn set(
        &mut self,
        id: TaskId,
        kind: TaskKind,
        message: impl Into<String>,
        progress: Option<f32>,
        now: Instant,
    ) {
        let message = message.into();
        let progress = progress.map(|p| p.clamp(0.0, 1.0));
        let done_at = (kind == TaskKind::Done).then_some(now);
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.kind = kind;
            task.message = message;
            task.progress = progress;
            task.done_at = done_at;
        } else {
            self.seq += 1;
            self.tasks.push(Task {
                id,
                kind,
                message,
                progress,
                started: self.seq,
                done_at,
            });
        }
    }

    pub fn running(&mut self, id: TaskId, message: impl Into<String>, progress: Option<f32>) {
        self.set(id, TaskKind::Running, message, progress, Instant::now());
    }

    pub fn attention(&mut self, id: TaskId, message: impl Into<String>) {
        self.set(id, TaskKind::Attention, message, None, Instant::now());
    }

    pub fn done(&mut self, id: TaskId, message: impl Into<String>) {
        self.set(id, TaskKind::Done, message, None, Instant::now());
    }

    /// Finish `id`: done, or needing attention if it failed.
    pub fn finish(&mut self, id: TaskId, result: Result<String, String>) {
        match result {
            Ok(message) => self.done(id, message),
            Err(message) => self.attention(id, message),
        }
    }

    /// A one-off message that clears itself like a finished task.
    pub fn note(&mut self, message: impl Into<String>) {
        let id = TaskId::Note(self.seq + 1);
        self.done(id, message);
    }

    pub fn clear(&mut self, id: TaskId) {
        self.tasks.retain(|t| t.id != id);
    }

    #[cfg(test)]
    pub fn get(&self, id: TaskId) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// Drop finished tasks that have been shown long enough.
    pub fn expire(&mut self, now: Instant) {
        self.tasks.retain(|t| {
            t.done_at
                .is_none_or(|at| now.saturating_duration_since(at) < DONE_FOR)
        });
    }

    /// How long until the next finished task expires.
    pub fn next_expiry(&self, now: Instant) -> Option<Duration> {
        self.tasks
            .iter()
            .filter_map(|t| t.done_at)
            .map(|at| (at + DONE_FOR).saturating_duration_since(now))
            .min()
    }

    /// Every task, most important first; within a kind, oldest first.
    pub fn sorted(&self) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self.tasks.iter().collect();
        tasks.sort_by_key(|t| (t.kind, t.started));
        tasks
    }

    /// The task the well shows.
    pub fn current(&self) -> Option<&Task> {
        self.tasks.iter().min_by_key(|t| (t.kind, t.started))
    }

    /// The others, for the `+N` key: none with one task or fewer.
    pub fn more(&self) -> usize {
        self.tasks.len().saturating_sub(1)
    }
}

/// The help the frame's widgets recorded: the one under the pointer, and
/// the keyboard-focused one.
#[derive(Clone, Default)]
struct HelpSlots {
    hover: Option<String>,
    focus: Option<String>,
}

fn help_id() -> Id {
    Id::unique("halo-status-help")
}

/// Help for a control, shown in the footer while it's under the pointer
/// or has keyboard focus. Disabled controls count while the pointer is
/// over them.
pub trait StatusHelp {
    fn status_help(self, text: impl Into<String>) -> Self;
}

impl StatusHelp for Response {
    fn status_help(self, text: impl Into<String>) -> Self {
        let hovered = self.hovered() || (!self.enabled() && self.contains_pointer());
        let focused = self.has_focus();
        if hovered || focused {
            let text = text.into();
            self.ctx.data_mut(|d| {
                let slots = d.get_temp_mut_or_default::<HelpSlots>(help_id());
                if focused {
                    slots.focus = Some(text.clone());
                }
                if hovered {
                    slots.hover = Some(text);
                }
            });
        }
        self
    }
}

/// The footer's state between frames.
#[derive(Default)]
pub struct Footer {
    pub tasks: Tasks,
    tray_open: bool,
    /// Where this frame's help goes, painted once the frame has said it.
    help: Option<(LayerId, Rect)>,
    /// The help painted last frame, to repaint once more when it changes.
    shown_help: Option<String>,
}

impl Footer {
    /// Clear last frame's help before the widgets record this frame's.
    pub fn begin_frame(ctx: &egui::Context) {
        ctx.data_mut(|d| d.remove::<HelpSlots>(help_id()));
    }

    /// The footer panel: places the help zone and draws the task well,
    /// and the tray above it while it's open. Call where the status line
    /// was, before the panels that sit above it.
    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = &ui.ctx().clone();
        let now = Instant::now();
        self.tasks.expire(now);
        if let Some(wait) = self.tasks.next_expiry(now) {
            ctx.request_repaint_after(wait);
        }
        if self.tasks.more() == 0 {
            self.tray_open = false;
        }
        let panel = egui::Panel::bottom("footer")
            .exact_size(FOOTER_H)
            .show_separator_line(false)
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.body(ui));
        if self.tray_open {
            self.tray(ctx, panel.response.rect);
        }
    }

    fn body(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        ui.allocate_rect(rect, Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, theme::PANEL);
        painter.hline(
            rect.x_range(),
            rect.top() + 0.5,
            Stroke::new(1.0, theme::EDGE_PANEL),
        );
        let inner = rect.shrink(PAD);
        let zone_top = inner.center().y - ZONE_H / 2.0;
        let well_w = WELL_W
            .min(inner.width() - ZONE_GAP - HELP_MIN_W)
            .max(WELL_MIN_W)
            .min(inner.width());
        let well =
            Rect::from_min_size(pos2(inner.right() - well_w, zone_top), vec2(well_w, ZONE_H));
        let help = Rect::from_min_max(
            pos2(inner.left(), zone_top),
            pos2(
                (well.left() - ZONE_GAP).max(inner.left()),
                zone_top + ZONE_H,
            ),
        );
        self.help = Some((ui.layer_id(), help));

        let more = self.tasks.more();
        let key_space = if more > 0 { MORE_KEY.x + ITEM_GAP } else { 0.0 };
        let current = self.tasks.current();
        paint_task(
            ui.painter(),
            well,
            current,
            WELL_PAD_RIGHT + key_space,
            true,
        );
        if more > 0 {
            let key = Rect::from_min_size(
                pos2(
                    well.right() - WELL_PAD_RIGHT - MORE_KEY.x,
                    well.center().y - MORE_KEY.y / 2.0,
                ),
                MORE_KEY,
            );
            let resp = ui.put(
                key,
                Key::new(format!("+{more}"))
                    .size(MORE_KEY)
                    .text_style(MORE_LABEL)
                    .lit(self.tray_open.then_some(Lit::Accent)),
            );
            if resp.status_help("Show all background tasks").clicked() {
                self.tray_open = !self.tray_open;
            }
        }
    }

    /// Every task in a well above the footer, right-aligned with it.
    fn tray(&mut self, ctx: &egui::Context, footer: Rect) {
        let screen = ctx.content_rect();
        let width = TRAY_W.min(screen.width() - 2.0 * TRAY_INSET).max(0.0);
        let tasks = self.tasks.sorted();
        let rows = tasks.len() as f32;
        let well_h = rows * TRAY_ROW_H + (rows - 1.0).max(0.0);
        egui::Area::new(Id::unique("footer-tray"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(pos2(footer.right() - TRAY_INSET, footer.top() - TRAY_GAP))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0, theme::EDGE))
                    .corner_radius(theme::RADIUS_WINDOW)
                    .inner_margin(TRAY_PAD)
                    .show(ui, |ui| {
                        let inner = width - 2.0 * TRAY_PAD;
                        let (well, _) = ui.allocate_exact_size(vec2(inner, well_h), Sense::hover());
                        let painter = ui.painter();
                        theme::paint_well(painter, well, theme::RADIUS_CONTROL);
                        for (i, task) in tasks.iter().enumerate() {
                            let top = well.top() + i as f32 * (TRAY_ROW_H + 1.0);
                            let row = Rect::from_min_size(
                                pos2(well.left(), top),
                                vec2(inner, TRAY_ROW_H),
                            );
                            paint_task(painter, row, Some(task), WELL_PAD_RIGHT, false);
                            if i + 1 < tasks.len() {
                                painter.hline(
                                    well.x_range(),
                                    row.bottom() + 0.5,
                                    Stroke::new(1.0, theme::QUIET_HAIRLINE),
                                );
                            }
                        }
                    });
            });
    }

    /// Paint the help the frame recorded into the help zone. Call last in
    /// the frame, once every widget has been laid out.
    pub fn paint_help(&mut self, ctx: &egui::Context) {
        let text = ctx.data(|d| {
            d.get_temp::<HelpSlots>(help_id())
                .and_then(|slots| slots.hover.or(slots.focus))
        });
        if let Some((layer, rect)) = self.help.take()
            && let Some(text) = &text
        {
            let painter = ctx.layer_painter(layer).with_clip_rect(rect);
            let galley = theme::truncated(
                ctx,
                text,
                theme::STATUS_HELP,
                theme::TEXT,
                rect.width() - 2.0 * HELP_PAD,
            );
            let line = Rect::from_center_size(
                pos2(rect.center().x, rect.center().y),
                vec2(rect.width(), theme::STATUS_HELP_LINE),
            );
            painter.galley(
                pos2(
                    rect.left() + HELP_PAD,
                    line.center().y - galley.size().y / 2.0,
                ),
                galley,
                theme::TEXT,
            );
        }
        // The help changed after the footer was painted: once more, so the
        // footer never keeps text the pointer has left.
        if text != self.shown_help {
            self.shown_help = text;
            ctx.request_repaint();
        }
    }
}

/// One task in a well or a tray row: its LED or warning glyph, its
/// message, and a progress strip when the progress is known. `None` is
/// the idle "Ready". `right` is what to keep clear at the right end.
fn paint_task(painter: &egui::Painter, rect: Rect, task: Option<&Task>, right: f32, well: bool) {
    let kind = task.map(|t| t.kind);
    if well {
        theme::paint_well(painter, rect, theme::RADIUS_CONTROL);
    }
    if kind == Some(TaskKind::Attention) {
        painter.rect_filled(
            rect,
            theme::RADIUS_CONTROL,
            theme::WARN.gamma_multiply(WARN_WASH),
        );
        painter.rect_stroke(
            rect,
            theme::RADIUS_CONTROL,
            Stroke::new(WARN_OUTLINE, theme::WARN),
            StrokeKind::Inside,
        );
    }
    let mid = rect.center().y;
    let mut x = rect.left() + WELL_PAD_LEFT;
    match kind {
        Some(TaskKind::Attention) => {
            paint_warning(painter, pos2(x + WARN_GLYPH / 2.0, mid), WARN_GLYPH);
            x += WARN_GLYPH;
        }
        Some(TaskKind::Running) => {
            painter.circle_filled(pos2(x + LED / 2.0, mid), LED / 2.0, theme::ACCENT);
            x += LED;
        }
        Some(TaskKind::Done) | None => {
            painter.circle_filled(pos2(x + LED / 2.0, mid), LED / 2.0, theme::LED_OFF);
            x += LED;
        }
    }
    x += ITEM_GAP;
    let (message, color) = match task {
        Some(t) if t.kind == TaskKind::Done => (t.message.as_str(), theme::TEXT),
        Some(t) => (t.message.as_str(), theme::TEXT_STRONG),
        None => ("Ready", theme::TEXT_MUTED),
    };
    let width = rect.right() - right - x;
    let galley = theme::truncated(painter.ctx(), message, theme::STATUS_MESSAGE, color, width);
    let line = theme::STATUS_MESSAGE_LINE.max(galley.size().y);
    painter.galley(
        pos2(x, mid - line / 2.0 + (line - galley.size().y) / 2.0),
        galley,
        color,
    );
    if let Some(progress) = task.and_then(|t| t.progress) {
        let track = Rect::from_min_max(
            pos2(
                rect.left() + STRIP_INSET,
                rect.bottom() - STRIP_BOTTOM - STRIP_H,
            ),
            pos2(rect.right() - STRIP_INSET, rect.bottom() - STRIP_BOTTOM),
        );
        painter.rect_filled(track, 0.0, theme::SEGMENT_OFF);
        let fill = Rect::from_min_max(
            track.min,
            pos2(track.left() + track.width() * progress, track.bottom()),
        );
        painter.rect_filled(fill, 0.0, theme::ACCENT);
    }
}

/// A warning triangle in `warn` with its exclamation mark cut from the
/// well, `size` across.
fn paint_warning(painter: &egui::Painter, center: Pos2, size: f32) {
    let h = size * 0.87;
    let top = pos2(center.x, center.y - h / 2.0);
    let left = pos2(center.x - size / 2.0, center.y + h / 2.0);
    let right = pos2(center.x + size / 2.0, center.y + h / 2.0);
    painter.add(egui::Shape::convex_polygon(
        vec![top, right, left],
        theme::WARN,
        Stroke::NONE,
    ));
    let mark = theme::WELL;
    let bar = Rect::from_min_max(
        pos2(center.x - size * 0.06, center.y - h * 0.15),
        pos2(center.x + size * 0.06, center.y + h * 0.2),
    );
    painter.rect_filled(bar, 0.0, mark);
    painter.circle_filled(pos2(center.x, center.y + h * 0.33), size * 0.065, mark);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attention_then_running_then_done_each_in_the_order_they_started() {
        let mut tasks = Tasks::default();
        let now = Instant::now();
        tasks.set(
            TaskId::Note(1),
            TaskKind::Done,
            "Imported 3 tracks",
            None,
            now,
        );
        tasks.set(
            TaskId::Load("A"),
            TaskKind::Running,
            "Deck A: loading…",
            None,
            now,
        );
        tasks.set(
            TaskId::Analysis,
            TaskKind::Running,
            "Analyzing…",
            Some(0.5),
            now,
        );
        assert_eq!(tasks.current().map(|t| t.id), Some(TaskId::Load("A")));
        tasks.set(TaskId::Patch, TaskKind::Attention, "2 fixtures", None, now);
        assert_eq!(tasks.current().map(|t| t.id), Some(TaskId::Patch));
        let order: Vec<_> = tasks.sorted().iter().map(|t| t.id).collect();
        assert_eq!(
            order,
            [
                TaskId::Patch,
                TaskId::Load("A"),
                TaskId::Analysis,
                TaskId::Note(1)
            ]
        );
        // Finishing keeps the load's place among finished tasks, and an
        // update keeps its start.
        tasks.set(
            TaskId::Load("A"),
            TaskKind::Done,
            "Deck A: Saucers",
            None,
            now,
        );
        tasks.set(
            TaskId::Analysis,
            TaskKind::Running,
            "Analyzing…",
            Some(0.75),
            now,
        );
        let order: Vec<_> = tasks.sorted().iter().map(|t| t.id).collect();
        assert_eq!(
            order,
            [
                TaskId::Patch,
                TaskId::Analysis,
                TaskId::Note(1),
                TaskId::Load("A")
            ]
        );
        assert_eq!(tasks.get(TaskId::Analysis).unwrap().progress, Some(0.75));
        tasks.clear(TaskId::Patch);
        assert_eq!(tasks.current().map(|t| t.id), Some(TaskId::Analysis));
    }

    #[test]
    fn done_tasks_expire_after_four_seconds() {
        let mut tasks = Tasks::default();
        let start = Instant::now();
        tasks.set(
            TaskId::Import,
            TaskKind::Done,
            "Imported 1 track",
            None,
            start,
        );
        tasks.set(
            TaskId::Analysis,
            TaskKind::Running,
            "Analyzing…",
            None,
            start,
        );
        assert_eq!(tasks.next_expiry(start), Some(DONE_FOR));
        tasks.expire(start + DONE_FOR - Duration::from_millis(1));
        assert!(tasks.get(TaskId::Import).is_some());
        tasks.expire(start + DONE_FOR);
        assert!(tasks.get(TaskId::Import).is_none());
        // Running and Attention tasks stay until their source says.
        tasks.expire(start + DONE_FOR * 10);
        assert!(tasks.get(TaskId::Analysis).is_some());
        assert_eq!(tasks.next_expiry(start), None);
        // Going back to running cancels the expiry.
        tasks.set(
            TaskId::Analysis,
            TaskKind::Done,
            "Analysis complete",
            None,
            start,
        );
        tasks.set(
            TaskId::Analysis,
            TaskKind::Running,
            "Analyzing…",
            None,
            start,
        );
        tasks.expire(start + DONE_FOR * 10);
        assert!(tasks.get(TaskId::Analysis).is_some());
    }

    #[test]
    fn more_counts_the_tasks_behind_the_shown_one() {
        let mut tasks = Tasks::default();
        assert_eq!(tasks.more(), 0);
        tasks.note("Removed Saucers from library");
        assert_eq!(tasks.more(), 0, "one task has no +N");
        tasks.note("Stored 2 cue(s) at bar 9");
        tasks.running(TaskId::Analysis, "Analyzing…", None);
        assert_eq!(tasks.more(), 2);
        // Notes are each their own task.
        assert_eq!(tasks.sorted().len(), 3);
        tasks.clear(TaskId::Analysis);
        assert_eq!(tasks.more(), 1);
    }
}

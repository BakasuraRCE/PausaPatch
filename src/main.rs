#![windows_subsystem = "windows"]

mod apply;
mod i18n;
mod log;
mod paths;
mod settings;
mod state;
mod tray;
mod win;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use windows_reactor::*;

use i18n::Lang;
use settings::Settings;
use state::{ChangeDetail, NoticeData, NoticeKind, Overall, SystemState};

const DEFAULT_POLL_SECONDS: u64 = 30;
const GREEN: (u8, u8, u8) = (0x6c, 0xcb, 0x5f);
const AMBER: (u8, u8, u8) = (0xff, 0xb9, 0x00);
const TEXT: (u8, u8, u8) = (0xed, 0xed, 0xed);
const MUTED: (u8, u8, u8) = (0x8a, 0x8a, 0x8a);
const CARD: (u8, u8, u8) = (0x26, 0x26, 0x26);
const CARD_BORDER: (u8, u8, u8) = (0x36, 0x36, 0x36);
const WINDOW_BG: (u8, u8, u8) = (0x20, 0x20, 0x20);

fn solid(rgb: (u8, u8, u8)) -> Brush {
    Brush::Solid(Color::rgb(rgb.0, rgb.1, rgb.2))
}

fn alpha(a: u8, rgb: (u8, u8, u8)) -> Brush {
    Brush::Solid(Color::argb(a, rgb.0, rgb.1, rgb.2))
}

fn text(value: impl Into<String>, size: f64, color: (u8, u8, u8)) -> TextBlock {
    TextBlock::new()
        .text(value.into())
        .font_size(size)
        .foreground(solid(color))
}

fn separator() -> Border {
    Border::new()
        .height(1.0)
        .background(solid((0x33, 0x33, 0x33)))
}

fn pill(label: impl Into<String>, fg: (u8, u8, u8), bg: (u8, u8, u8)) -> Border {
    Border::new()
        .background(alpha(34, bg))
        .corner_radius(11.0)
        .padding(Thickness::xy(10.0, 3.0))
        .vertical_alignment(VerticalAlignment::Center)
        .content(
            TextBlock::new()
                .text(label.into())
                .font_size(11.0)
                .foreground(solid(fg)),
        )
}

fn status_row(dot: (u8, u8, u8), title: &str, subtitle: String, pill_view: Border) -> Border {
    Border::new()
        .padding(Thickness::new(16.0, 8.0, 16.0, 8.0))
        .content(
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                .column_spacing(10.0)
                .children((
                    Ellipse::new()
                        .width(7.0)
                        .height(7.0)
                        .fill(solid(dot))
                        .grid_column(0)
                        .vertical_alignment(VerticalAlignment::Center),
                    StackPanel::new().spacing(1.0).grid_column(1).children((
                        text(title, 13.0, TEXT),
                        TextBlock::new()
                            .text(subtitle)
                            .font_size(10.5)
                            .foreground(solid(MUTED))
                            .max_lines(1)
                            .text_trimming(TextTrimming::CharacterEllipsis),
                    )),
                    pill_view
                        .grid_column(2)
                        .vertical_alignment(VerticalAlignment::Center),
                )),
        )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoticeAction {
    Dismiss,
    PauseAgain,
}

#[derive(Clone)]
enum Message {
    Poll,
    StateReady(Result<SystemState, String>),
    Toggle(bool),
    OperationDone {
        action: apply::Action,
        result: Result<(), String>,
    },
    Language(Option<usize>),
    Autostart(bool),
    NoticeAction(NoticeAction),
    ShowNextNotice,
    DismissError,
    OpenLog,
    OpenAbout,
    AboutClosed,
    CheckNow,
    Minimize,
    TrayTick,
    Noop,
}

struct PausaPatch {
    lang: Lang,
    settings: Settings,
    state: Option<SystemState>,
    last_check: Option<i64>,
    last_change: Option<i64>,
    error: Option<String>,
    busy: bool,
    desired_toggle: Option<bool>,
    autostart: bool,
    elevated: bool,
    about_open: bool,
    notice_open: bool,
    pending_notices: VecDeque<NoticeData>,
    ignore_next_notice: bool,
    verify_target: Option<Overall>,
    verify_deadline: Option<Instant>,
    deferred_error: Option<String>,
    check_in_flight: bool,
    poll_seconds: u64,
    poll_timer: Option<ComponentTimer>,
    minimize_timer: Option<ComponentTimer>,
    next_notice_timer: Option<ComponentTimer>,
    tray_timer: Option<ComponentTimer>,
    window_configured: bool,
    start_minimized: bool,
}

impl PausaPatch {
    fn start_check(&mut self, context: &ComponentContext<Self>) {
        self.poll_timer = Some(context.set_local_timeout(
            Duration::from_secs(self.poll_seconds),
            || Message::Poll,
        ));
        if self.check_in_flight {
            return;
        }
        self.check_in_flight = true;
        context.spawn_background(|_| Message::StateReady(state::detect()));
    }

    fn begin_action(&mut self, action: apply::Action, context: &ComponentContext<Self>) {
        if self.busy {
            return;
        }
        if !self.elevated {
            self.error = Some(self.lang.strings().admin_required.to_string());
            return;
        }
        self.busy = true;
        self.desired_toggle = Some(action == apply::Action::Pause);
        self.verify_target = None;
        self.verify_deadline = None;
        self.deferred_error = None;
        self.error = None;
        self.ignore_next_notice = true;
        log::log(format!("Requested action: {}", action.as_str()));
        context.spawn_background(move |_| Message::OperationDone {
            action,
            result: apply::apply(action),
        });
    }

    fn open_next_notice(&mut self, context: &ComponentContext<Self>) {
        if self.notice_open {
            return;
        }
        let Some(notice) = self.pending_notices.pop_front() else {
            return;
        };
        let input = NoticeInput {
            lang: self.lang,
            notice: notice.clone(),
            on_action: context.callback(Message::NoticeAction),
        };
        if context.open_window::<NoticeWindow>(input) {
            self.notice_open = true;
        } else {
            self.pending_notices.push_front(notice);
        }
    }

    fn overall(&self) -> Overall {
        self.state
            .as_ref()
            .map(|value| value.overall)
            .unwrap_or_default()
    }

    fn tray_state(&self) -> tray::TrayState {
        let s = self.lang.strings();
        let (status, paused) = match self.overall() {
            Overall::Paused => (s.paused_title, Some(true)),
            Overall::Active => (s.active_title, Some(false)),
            Overall::Mixed => (s.mixed_title, Some(false)),
            Overall::Unknown => (s.unknown_title, None),
        };
        tray::TrayState {
            show: s.tray_show.to_string(),
            pause: s.tray_pause.to_string(),
            resume: s.tray_resume.to_string(),
            exit: s.tray_exit.to_string(),
            tooltip: format!("PausaPatch — {status}"),
            paused,
        }
    }

    fn meta_line(&self, lang: Lang) -> String {
        if let Some(change) = self.last_change {
            let date = lang.format_date(change);
            return match self.overall() {
                Overall::Paused => lang.paused_since(&date),
                Overall::Active => lang.reactivated_since(&date),
                Overall::Mixed => lang.mixed_since(&date),
                Overall::Unknown => lang.checked_at(&date),
            };
        }
        if let Some(checked) = self.last_check {
            return lang.checked_at(&lang.format_date(checked));
        }
        String::new()
    }

    fn queue_notice(&mut self, kind: NoticeKind, detail: ChangeDetail, at: i64) {
        self.pending_notices
            .push_back(NoticeData { kind, detail, at });
    }

    fn handle_state(&mut self, new_state: SystemState, context: &ComponentContext<Self>) {
        // Mientras una acción propia está en curso o pendiente de verificar,
        // ningún cambio detectado debe anunciarse como externo.
        if self.busy || self.verify_target.is_some() {
            self.ignore_next_notice = true;
        }
        let previous = self.state.replace(new_state.clone());
        self.last_check = Some(new_state.checked_at);

        if new_state.overall == Overall::Unknown {
            if !self.busy && self.verify_target.is_none() {
                self.ignore_next_notice = false;
            }
            if self.verify_target.is_some() {
                self.poll_timer = Some(context.set_local_timeout(
                    Duration::from_millis(1500),
                    || Message::Poll,
                ));
            }
            self.open_next_notice(context);
            return;
        }

        let code = overall_code(new_state.overall).to_string();
        if let Some(previous) = previous {
            if previous.overall != Overall::Unknown && previous.overall != new_state.overall {
                let now = new_state.checked_at;
                self.last_change = Some(now);
                self.settings.last_change = Some(now);
                self.settings.last_overall = Some(code);
                let _ = self.settings.save();
                log::log(format!(
                    "State change: {:?} -> {:?}",
                    previous.overall, new_state.overall
                ));
                if !self.ignore_next_notice
                    && let Some(kind) = notice_kind(new_state.overall)
                {
                    self.queue_notice(kind, state::change_detail(&previous, &new_state), now);
                }
            } else {
                self.settings.last_overall = Some(code);
                let _ = self.settings.save();
            }
        } else {
            let saved = self
                .settings
                .last_overall
                .as_deref()
                .and_then(overall_from_code);
            match saved {
                Some(saved) if saved != Overall::Unknown && saved != new_state.overall => {
                    let now = new_state.checked_at;
                    self.last_change = Some(now);
                    self.settings.last_change = Some(now);
                    self.settings.last_overall = Some(code);
                    let _ = self.settings.save();
                    log::log(format!(
                        "Change detected since last start: {:?} -> {:?}",
                        saved, new_state.overall
                    ));
                    if !self.ignore_next_notice
                        && let Some(kind) = notice_kind(new_state.overall)
                    {
                        self.queue_notice(kind, ChangeDetail::Generic, now);
                    }
                }
                Some(_) => {
                    self.settings.last_overall = Some(code);
                    let _ = self.settings.save();
                }
                None => {
                    self.last_change = None;
                    self.settings.last_change = None;
                    self.settings.last_overall = Some(code);
                    let _ = self.settings.save();
                }
            }
        }

        tray::set_state(self.tray_state());
        if let Some(target) = self.verify_target {
            if new_state.overall == target {
                // Confirmado: la acción surtió efecto.
                self.verify_target = None;
                self.verify_deadline = None;
                self.desired_toggle = None;
                self.deferred_error = None;
                self.error = None;
                self.busy = false;
                self.ignore_next_notice = false;
                log::log("Verification complete: state confirmed.");
            } else if self
                .verify_deadline
                .is_some_and(|deadline| Instant::now() < deadline)
            {
                // Windows todavía puede estar aplicando los cambios.
                self.ignore_next_notice = true;
                self.poll_timer = Some(context.set_local_timeout(
                    Duration::from_millis(1500),
                    || Message::Poll,
                ));
            } else {
                self.verify_target = None;
                self.verify_deadline = None;
                self.desired_toggle = None;
                self.busy = false;
                self.ignore_next_notice = false;
                let detail = self.deferred_error.take();
                self.error = Some(match detail {
                    Some(detail) => format!("{} {detail}", self.lang.strings().operation_failed),
                    None => self.lang.strings().operation_failed.to_string(),
                });
            }
        } else if !self.busy {
            self.ignore_next_notice = false;
        }
        self.open_next_notice(context);
    }
}

impl Component for PausaPatch {
    type Input = ();
    type Message = Message;

    fn create(_input: &Self::Input, context: &ComponentContext<Self>) -> Self {
        let _ = paths::ensure_dirs();
        log::log(format!(
            "PausaPatch {} started · {}",
            env!("CARGO_PKG_VERSION"),
            paths::exe_path().display()
        ));
        let settings = Settings::load();
        let lang = settings
            .language
            .as_deref()
            .and_then(Lang::from_code)
            .unwrap_or_else(Lang::detect);
        let poll_seconds = settings
            .poll_seconds
            .unwrap_or(DEFAULT_POLL_SECONDS)
            .clamp(5, 3600);
        let elevated = win::is_elevated();
        let mut app = Self {
            lang,
            last_change: settings.last_change,
            settings,
            state: None,
            last_check: None,
            error: if elevated {
                None
            } else {
                Some(lang.strings().admin_required.to_string())
            },
            busy: false,
            desired_toggle: None,
            autostart: win::is_autostart_enabled(),
            elevated,
            about_open: false,
            notice_open: false,
            pending_notices: VecDeque::new(),
            ignore_next_notice: false,
            verify_target: None,
            verify_deadline: None,
            deferred_error: None,
            check_in_flight: false,
            poll_seconds,
            poll_timer: None,
            minimize_timer: None,
            next_notice_timer: None,
            tray_timer: None,
            window_configured: false,
            start_minimized: std::env::args().any(|argument| argument == "--minimized"),
        };
        app.poll_timer =
            Some(context.set_local_timeout(Duration::from_millis(350), || Message::Poll));
        if app.start_minimized {
            app.minimize_timer =
                Some(context.set_local_timeout(Duration::from_millis(300), || Message::Minimize));
        }
        tray::set_state(app.tray_state());
        tray::start();
        app.tray_timer =
            Some(context.set_local_timeout(Duration::from_millis(400), || Message::TrayTick));
        app
    }

    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        match message {
            Message::Poll => self.start_check(context),
            Message::CheckNow => self.start_check(context),
            Message::StateReady(result) => {
                self.check_in_flight = false;
                match result {
                    Ok(new_state) => self.handle_state(new_state, context),
                    Err(error) => {
                        log::log(format!("Detection error: {error}"));
                        if self.state.is_none() {
                            self.error =
                                Some(format!("{} {error}", self.lang.strings().check_failed));
                        }
                        if self.verify_target.is_some() {
                            self.poll_timer = Some(context.set_local_timeout(
                                Duration::from_millis(1500),
                                || Message::Poll,
                            ));
                        }
                    }
                }
            }
            Message::Toggle(value) => {
                if self.busy || self.state.is_none() {
                    return;
                }
                let action = if value {
                    apply::Action::Pause
                } else {
                    apply::Action::Resume
                };
                self.begin_action(action, context);
            }
            Message::OperationDone { action, result } => {
                // El estado deseado se mantiene hasta que la comprobación lo
                // confirme; así el toggle no retrocede mientras se aplica.
                self.verify_target = Some(if action == apply::Action::Pause {
                    Overall::Paused
                } else {
                    Overall::Active
                });
                self.verify_deadline = Some(Instant::now() + Duration::from_secs(25));
                match result {
                    Ok(()) => self.deferred_error = None,
                    Err(error) => {
                        // Un paso puede fallar aunque el estado final sea el
                        // deseado; la comprobación posterior decide.
                        log::log(format!("Action {} returned an error: {error}", action.as_str()));
                        self.deferred_error = Some(error);
                    }
                }
                self.poll_timer =
                    Some(context.set_local_timeout(Duration::from_millis(1500), || Message::Poll));
            }
            Message::Language(index) => {
                if let Some(index) = index
                    && let Some(lang) = Lang::ALL.get(index).copied()
                {
                    self.lang = lang;
                    self.settings.language = Some(lang.code().to_string());
                    let _ = self.settings.save();
                    tray::set_state(self.tray_state());
                }
            }
            Message::Autostart(enabled) => match win::set_autostart(enabled) {
                Ok(()) => {
                    self.autostart = enabled;
                    log::log(format!("Start with Windows: {enabled}"));
                }
                Err(error) => {
                    self.error = Some(format!("{} {error}", self.lang.strings().error_generic));
                }
            },
            Message::NoticeAction(action) => {
                self.notice_open = false;
                if action == NoticeAction::PauseAgain && !self.busy {
                    self.begin_action(apply::Action::Pause, context);
                }
                self.next_notice_timer = Some(
                    context
                        .set_local_timeout(Duration::from_millis(400), || Message::ShowNextNotice),
                );
            }
            Message::ShowNextNotice => self.open_next_notice(context),
            Message::DismissError => self.error = None,
            Message::OpenAbout => self.about_open = true,
            Message::AboutClosed => self.about_open = false,
            Message::OpenLog => {
                let input = LogInput {
                    lang: self.lang,
                    lines: log::read(400),
                };
                let _ = context.open_window::<LogWindow>(input);
            }
            Message::Minimize => {
                if tray::is_available() {
                    let _ = context.run_window(|handle| {
                        win::hide_window(handle.as_raw() as _);
                        Message::Noop
                    });
                }
            }
            Message::TrayTick => {
                self.tray_timer =
                    Some(context.set_local_timeout(Duration::from_millis(400), || {
                        Message::TrayTick
                    }));
                if !self.window_configured {
                    self.window_configured = true;
                    let tray_available = tray::is_available();
                    let _ = context.run_window(move |handle| {
                        win::configure_main_window(handle.as_raw() as _, tray_available);
                        Message::Noop
                    });
                }
                for command in tray::drain() {
                    match command {
                        tray::Command::Show => {
                            let _ = context.run_window(|handle| {
                                win::restore_window(handle.as_raw() as _);
                                Message::Noop
                            });
                        }
                        tray::Command::Toggle => {
                            // Un ContentDialog abierto no es una ventana aparte:
                            // se cierra aquí y después se alterna la principal.
                            self.about_open = false;
                            let _ = context.run_window(|handle| {
                                win::toggle_window(handle.as_raw() as _);
                                Message::Noop
                            });
                        }
                        tray::Command::Pause => self.begin_action(apply::Action::Pause, context),
                        tray::Command::Resume => self.begin_action(apply::Action::Resume, context),
                        tray::Command::Exit => {
                            let _ = context.run_window(|_| {
                                win::quit_application("PausaPatch");
                                Message::Noop
                            });
                        }
                    }
                }
            }
            Message::Noop => {}
        }
    }

    fn view(&self, _input: &Self::Input, context: &mut ViewContext<Self>) -> View {
        let mut visuals = WindowVisuals::new()
            .client_size(940.0, 760.0)
            .theme(WindowTheme::Dark)
            .backdrop(WindowBackdrop::MicaAlt)
            .constraints(WindowConstraints {
                min_width: Some(900.0),
                min_height: Some(620.0),
                max_width: None,
                max_height: None,
            });
        // Reactor muestra la ventana al crearla: si hay que arrancar en la
        // bandeja, se crea fuera de pantalla y se oculta, sin parpadeo.
        // Si no, se abre centrada en el monitor activo.
        if self.start_minimized {
            visuals = visuals.initial_position(ScreenPoint::new(-32_000, -32_000));
        } else {
            let (x, y) = win::centered_position(940.0, 760.0);
            visuals = visuals.initial_position(ScreenPoint::new(x, y));
        }
        context.window_visuals(visuals);
        let content = self.main_view(context);
        context.window_frame("PausaPatch", content)
    }
}

impl PausaPatch {
    fn main_view(&self, context: &mut ViewContext<Self>) -> View {
        let s = self.lang.strings();
        let mut sections: Vec<View> = Vec::new();
        if let Some(error) = &self.error {
            sections.push(
                InfoBar::new()
                    .severity(InfoBarSeverity::Error)
                    .title(s.error_generic)
                    .message(error)
                    .is_open(true)
                    .is_closable(true)
                    .on_closed(context.message(Message::DismissError))
                    .into(),
            );
        }
        sections.push(self.header(context));
        sections.push(self.hero(context));
        sections.push(self.status_section(context));
        sections.push(self.options_section(context));
        sections.push(
            separator()
                .margin(Thickness::new(0.0, 2.0, 0.0, 0.0))
                .into(),
        );
        sections.push(self.footer(context));

        ScrollView::new()
            .content(
                Border::new()
                    .padding(Thickness::new(28.0, 20.0, 28.0, 18.0))
                    .background(solid(WINDOW_BG))
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .content(StackPanel::new().spacing(16.0).children(sections))
                    .content_dialog(self.about_dialog(context)),
            )
            .into()
    }

    fn header(&self, context: &mut ViewContext<Self>) -> View {
        let s = self.lang.strings();
        // La versión vive en «Acerca de»; aquí basta con el nombre.
        let left = StackPanel::new().spacing(2.0).children((
            TextBlock::new()
                .text("PausaPatch")
                .font_size(26.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(solid((0xff, 0xff, 0xff))),
            TextBlock::new()
                .text(s.app_subtitle)
                .font_size(12.5)
                .foreground(solid((0xa6, 0xa6, 0xa6)))
                .text_wrapping(TextWrapping::WrapWholeWords),
        ));

        let language = ComboBox::new()
            .items_source(Lang::ALL.iter().map(|lang| lang.native_name()))
            .selected_index(Some(self.lang.index()))
            .width(150.0)
            .automation_name(s.language)
            .on_selection_changed(context.callback(Message::Language));

        let badge_color = if self.elevated { GREEN } else { AMBER };
        let badge = Border::new()
            .background(solid((0x26, 0x26, 0x26)))
            .border_brush(solid((0x3a, 0x3a, 0x3a)))
            .border_thickness(1.0)
            .corner_radius(14.0)
            .padding(Thickness::xy(10.0, 5.0))
            .content(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(6.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        Ellipse::new()
                            .width(7.0)
                            .height(7.0)
                            .fill(solid(badge_color)),
                        TextBlock::new()
                            .text(if self.elevated {
                                s.session_elevated
                            } else {
                                s.session_standard
                            })
                            .font_size(11.5)
                            .foreground(solid((0xc8, 0xc8, 0xc8))),
                    )),
            );

        let right = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(10.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((language, badge));

        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((
                left.grid_column(0),
                right
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center),
            ))
            .into()
    }

    fn hero(&self, context: &mut ViewContext<Self>) -> View {
        let s = self.lang.strings();
        let overall = self.overall();
        let (accent, glyph, title, subtitle) = match overall {
            Overall::Paused => (GREEN, "✓", s.paused_title, s.paused_sub),
            Overall::Active => (AMBER, "!", s.active_title, s.active_sub),
            Overall::Mixed => (AMBER, "!", s.mixed_title, s.mixed_sub),
            Overall::Unknown => (MUTED, "?", s.unknown_title, s.unknown_sub),
        };

        let icon = Border::new()
            .width(54.0)
            .height(54.0)
            .corner_radius(27.0)
            .background(alpha(34, accent))
            .vertical_alignment(VerticalAlignment::Center)
            .content(
                TextBlock::new()
                    .text(glyph)
                    .font_size(24.0)
                    .font_weight(FontWeight::BOLD)
                    .foreground(solid(accent))
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .vertical_alignment(VerticalAlignment::Center),
            );

        let meta = self.meta_line(self.lang);
        let mut text_children: Vec<View> = vec![
            TextBlock::new()
                .text(title)
                .font_size(22.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(solid((0xff, 0xff, 0xff)))
                .text_wrapping(TextWrapping::WrapWholeWords)
                .into(),
            TextBlock::new()
                .text(subtitle)
                .font_size(12.5)
                .foreground(solid((0xaf, 0xaf, 0xaf)))
                .text_wrapping(TextWrapping::WrapWholeWords)
                .into(),
        ];
        if !meta.is_empty() {
            text_children.push(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(7.0)
                    .children((
                        Ellipse::new().width(5.0).height(5.0).fill(solid(accent)),
                        TextBlock::new()
                            .text(meta)
                            .font_size(11.5)
                            .foreground(solid((0x8f, 0x8f, 0x8f))),
                    ))
                    .into(),
            );
        }
        let text_stack = StackPanel::new()
            .spacing(4.0)
            .grid_column(1)
            .vertical_alignment(VerticalAlignment::Center)
            .children(text_children);

        let toggle_on = self.desired_toggle.unwrap_or(overall == Overall::Paused);
        let toggle_label = if self.busy {
            s.applying
        } else {
            match overall {
                Overall::Paused => s.protection_on,
                Overall::Active => s.protection_off,
                Overall::Mixed => s.protection_mixed,
                Overall::Unknown => s.protection_mixed,
            }
        };
        let toggle = ToggleSwitch::new()
            .is_on(toggle_on)
            .is_enabled(!self.busy && self.state.is_some() && self.elevated)
            .header(
                TextBlock::new()
                    .text(toggle_label)
                    .font_size(13.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(solid(if overall == Overall::Paused {
                        GREEN
                    } else {
                        accent
                    })),
            )
            .on_toggled(context.callback(Message::Toggle));
        let toggle_stack = StackPanel::new()
            .spacing(4.0)
            .grid_column(2)
            .horizontal_alignment(HorizontalAlignment::Right)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                toggle,
                TextBlock::new()
                    .text(self.lang.check_interval(self.poll_seconds))
                    .font_size(10.5)
                    .foreground(solid(MUTED))
                    .horizontal_alignment(HorizontalAlignment::Right),
            ));

        Border::new()
            .background(solid((0x2c, 0x2c, 0x2c)))
            .border_brush(solid((0x3a, 0x3a, 0x3a)))
            .border_thickness(1.0)
            .corner_radius(14.0)
            .padding(24.0)
            .content(
                Grid::new()
                    .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                    .column_spacing(20.0)
                    .children((icon.grid_column(0), text_stack, toggle_stack)),
            )
            .into()
    }

    fn status_section(&self, context: &mut ViewContext<Self>) -> View {
        let s = self.lang.strings();
        let state = self.state.as_ref();
        let mut rows: Vec<View> = Vec::new();

        let wuauserv = state.and_then(|value| value.wuauserv);
        rows.push(
            status_row(
                if wuauserv == Some(4) { GREEN } else { AMBER },
                s.service_wuauserv,
                "wuauserv".to_string(),
                pill(
                    self.lang.service_state_label(wuauserv),
                    if wuauserv == Some(4) { GREEN } else { AMBER },
                    if wuauserv == Some(4) { GREEN } else { AMBER },
                ),
            )
            .into(),
        );
        rows.push(separator().into());

        let uso = state.and_then(|value| value.uso);
        rows.push(
            status_row(
                if uso == Some(4) { GREEN } else { AMBER },
                s.service_uso,
                "UsoSvc".to_string(),
                pill(
                    self.lang.service_state_label(uso),
                    if uso == Some(4) { GREEN } else { AMBER },
                    if uso == Some(4) { GREEN } else { AMBER },
                ),
            )
            .into(),
        );
        rows.push(separator().into());

        let medic = state.and_then(|value| value.medic);
        let renamed = state.is_some_and(|value| value.medic_dll_renamed);
        rows.push(
            status_row(
                if medic == Some(4) { GREEN } else { AMBER },
                s.service_medic,
                format!(
                    "WaaSMedicSvc · {}",
                    if renamed {
                        s.dll_renamed
                    } else {
                        s.dll_restored
                    }
                ),
                pill(
                    self.lang.service_state_label(medic),
                    if medic == Some(4) { GREEN } else { AMBER },
                    if medic == Some(4) { GREEN } else { AMBER },
                ),
            )
            .into(),
        );
        rows.push(separator().into());

        let (tasks_total, tasks_disabled) = state
            .map(|value| (value.tasks_total, value.tasks_disabled))
            .unwrap_or((0, 0));
        let tasks_enabled = tasks_total.saturating_sub(tasks_disabled);
        let all_disabled = tasks_total > 0 && tasks_disabled >= tasks_total;
        let all_enabled = tasks_total > 0 && tasks_disabled == 0;
        // Verde solo cuando las tareas están bloqueando actualizaciones,
        // igual que el resto de filas: con la protección off, ámbar.
        let tasks_ok = all_disabled;
        let tasks_label = if tasks_total == 0 {
            s.unconfigured.to_string()
        } else if all_disabled {
            self.lang.tasks_disabled(tasks_disabled as usize)
        } else if all_enabled {
            self.lang.tasks_enabled(tasks_enabled as usize)
        } else {
            // El orden muestra primero lo anómalo: con la protección activa,
            // las que siguen habilitadas; con la protección off, las deshabilitadas.
            self.lang.tasks_partial(
                tasks_enabled as usize,
                tasks_disabled as usize,
                self.overall() == Overall::Paused,
            )
        };
        rows.push(
            status_row(
                if tasks_ok { GREEN } else { AMBER },
                s.tasks,
                "InstallService, UpdateOrchestrator, WaaSMedic, WindowsUpdate…".to_string(),
                pill(
                    tasks_label,
                    if tasks_ok { GREEN } else { AMBER },
                    if tasks_ok { GREEN } else { AMBER },
                ),
            )
            .into(),
        );
        rows.push(separator().into());

        let policy = state.and_then(|value| value.policy);
        let (policy_label, policy_color) = match policy {
            Some(1) => (s.enabled, GREEN),
            Some(0) => (s.policy_off, AMBER),
            _ => (s.unconfigured, AMBER),
        };
        rows.push(
            status_row(
                policy_color,
                s.policy,
                r"HKLM\Software\Policies\Microsoft\Windows\WindowsUpdate\AU".to_string(),
                pill(policy_label, policy_color, policy_color),
            )
            .into(),
        );

        let heading = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((
                TextBlock::new()
                    .text(s.state_heading)
                    .font_size(15.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(solid((0xf0, 0xf0, 0xf0)))
                    .grid_column(0),
                Button::new()
                    .style(ButtonStyle::TextLink)
                    .content(s.refresh_now)
                    .is_enabled(!self.busy)
                    .grid_column(1)
                    .on_click(context.message(Message::CheckNow)),
            ));

        StackPanel::new()
            .spacing(8.0)
            .children((
                heading,
                Border::new()
                    .background(solid(CARD))
                    .border_brush(solid(CARD_BORDER))
                    .border_thickness(1.0)
                    .corner_radius(12.0)
                    .content(StackPanel::new().spacing(0.0).children(rows)),
            ))
            .into()
    }

    fn options_section(&self, context: &mut ViewContext<Self>) -> View {
        let s = self.lang.strings();
        let autostart_row = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(16.0)
            .children((
                StackPanel::new().spacing(1.0).grid_column(0).children((
                    text(s.start_with_windows, 13.0, TEXT),
                    TextBlock::new()
                        .text(s.start_with_windows_desc)
                        .font_size(10.5)
                        .foreground(solid(MUTED))
                        .text_wrapping(TextWrapping::WrapWholeWords),
                )),
                ToggleSwitch::new()
                    .is_on(self.autostart)
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .on_toggled(context.callback(Message::Autostart)),
            ));

        Border::new()
            .background(solid(CARD))
            .border_brush(solid(CARD_BORDER))
            .border_thickness(1.0)
            .corner_radius(12.0)
            .content(
                StackPanel::new().spacing(0.0).children((
                    Border::new()
                        .padding(Thickness::new(16.0, 8.0, 16.0, 8.0))
                        .content(autostart_row),
                )),
            )
            .into()
    }

    fn footer(&self, context: &mut ViewContext<Self>) -> View {
        let s = self.lang.strings();
        let checked = self
            .last_check
            .map(|value| self.lang.checked_at(&self.lang.format_date(value)))
            .unwrap_or_default();
        let status_text = if checked.is_empty() {
            s.watching.to_string()
        } else {
            format!("{} · {}", s.watching, checked)
        };
        let dot = match self.overall() {
            Overall::Paused => GREEN,
            Overall::Active | Overall::Mixed => AMBER,
            Overall::Unknown => MUTED,
        };

        Grid::new()
            .columns([
                GridLength::STAR,
                GridLength::Auto,
                GridLength::Auto,
                GridLength::Auto,
            ])
            .column_spacing(8.0)
            .children((
                Grid::new()
                    .columns([GridLength::Auto, GridLength::STAR])
                    .column_spacing(7.0)
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        Ellipse::new()
                            .width(7.0)
                            .height(7.0)
                            .fill(solid(dot))
                            .grid_column(0),
                        TextBlock::new()
                            .text(status_text)
                            .font_size(11.5)
                            .foreground(solid((0x9a, 0x9a, 0x9a)))
                            .max_lines(1)
                            .text_trimming(TextTrimming::CharacterEllipsis)
                            .grid_column(1),
                    )),
                Button::new()
                    .content(s.about)
                    .grid_column(1)
                    .on_click(context.message(Message::OpenAbout)),
                Button::new()
                    .content(s.log)
                    .grid_column(2)
                    .on_click(context.message(Message::OpenLog)),
                Button::new()
                    .content(s.check_now)
                    .is_enabled(!self.busy)
                    .grid_column(3)
                    .on_click(context.message(Message::CheckNow)),
            ))
            .into()
    }

    fn about_dialog(&self, context: &mut ViewContext<Self>) -> ContentDialog {
        let s = self.lang.strings();
        // Una sola rejilla: la columna Auto de las etiquetas se calcula con la
        // más ancha, así los valores quedan alineados entre filas.
        let label = |text: &'static str, row: i32| {
            TextBlock::new()
                .text(text)
                .font_size(12.0)
                .foreground(solid(MUTED))
                .grid_column(0)
                .grid_row(row)
                .vertical_alignment(VerticalAlignment::Center)
        };
        let value = |text: &'static str, row: i32| {
            TextBlock::new()
                .text(text)
                .font_size(12.0)
                .foreground(solid(TEXT))
                .grid_column(1)
                .grid_row(row)
                .vertical_alignment(VerticalAlignment::Center)
        };
        let link = |text: &'static str, url: &'static str, row: i32| {
            let button = match HyperlinkButton::new().navigate_uri(url) {
                Ok(button) => button,
                Err(_) => HyperlinkButton::new(),
            };
            button
                .content(text)
                .grid_column(1)
                .grid_row(row)
                // WinUI deja un sangrado interno en el HyperlinkButton que el
                // control no permite ajustar; se compensa con margen negativo.
                .margin(Thickness::new(-14.0, 0.0, 0.0, 0.0))
                .vertical_alignment(VerticalAlignment::Center)
        };

        let details = Grid::new()
            .columns([GridLength::Auto, GridLength::STAR])
            .rows([
                GridLength::Auto,
                GridLength::Auto,
                GridLength::Auto,
                GridLength::Auto,
                GridLength::Auto,
            ])
            .column_spacing(16.0)
            .row_spacing(7.0)
            .children((
                label(s.about_project, 0),
                link(
                    "github.com/BakasuraRCE/PausaPatch",
                    "https://github.com/BakasuraRCE/PausaPatch",
                    0,
                ),
                label(s.about_author_page, 1),
                link(
                    "bakasura.im",
                    "https://bakasura.im/?utm_source=pausapatch&utm_medium=app&utm_campaign=about&utm_content=author",
                    1,
                ),
                label(s.about_author, 2),
                value("bakasura", 2),
                label(s.about_license, 3),
                value("MIT", 3),
                label(s.about_built_with, 4),
                value("Rust · WinUI 3 · windows-reactor", 4),
            ));

        let body = StackPanel::new().spacing(10.0).children((
            TextBlock::new()
                .text(format!("PausaPatch v{}", env!("CARGO_PKG_VERSION")))
                .font_size(18.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(solid((0xff, 0xff, 0xff))),
            TextBlock::new()
                .text(s.about_tagline)
                .font_size(12.5)
                .foreground(solid((0xa8, 0xa8, 0xa8)))
                .text_wrapping(TextWrapping::WrapWholeWords),
            separator(),
            details,
        ));

        ContentDialog::new()
            .title(s.about)
            .content(body)
            .close_button_text(s.close)
            .default_button(ContentDialogButton::Close)
            .is_open(self.about_open)
            .on_closed(context.callback(|_| Message::AboutClosed))
    }
}

#[derive(Clone, PartialEq)]
struct NoticeInput {
    lang: Lang,
    notice: NoticeData,
    on_action: Callback<NoticeAction>,
}

#[derive(Clone, Copy)]
enum NoticeMessage {
    Styled,
    Dismiss,
    PauseAgain,
}

struct NoticeWindow {
    lang: Lang,
    notice: NoticeData,
    on_action: Callback<NoticeAction>,
    notified: bool,
}

impl NoticeWindow {
    fn notify(&mut self, action: NoticeAction) {
        if !self.notified {
            self.notified = true;
            self.on_action.call(action);
        }
    }
}

impl Component for NoticeWindow {
    type Input = NoticeInput;
    type Message = NoticeMessage;

    fn create(input: &Self::Input, context: &ComponentContext<Self>) -> Self {
        let bounds = win::popup_bounds(420.0, 236.0, 12.0);
        let _ = context.run_window(move |handle| {
            win::hide_on_close(handle.as_raw() as _);
            win::configure_popup(handle.as_raw() as _, bounds);
            NoticeMessage::Styled
        });
        Self {
            lang: input.lang,
            notice: input.notice.clone(),
            on_action: input.on_action.clone(),
            notified: false,
        }
    }

    fn input_changed(&mut self, input: &Self::Input, _context: &ComponentContext<Self>) {
        self.lang = input.lang;
        self.notice = input.notice.clone();
        self.on_action = input.on_action.clone();
    }

    fn update(&mut self, message: NoticeMessage, context: &ComponentContext<Self>) {
        match message {
            NoticeMessage::Styled => {}
            NoticeMessage::Dismiss => {
                self.notify(NoticeAction::Dismiss);
                let _ = context.close_window();
            }
            NoticeMessage::PauseAgain => {
                self.notify(NoticeAction::PauseAgain);
                let _ = context.close_window();
            }
        }
    }

    fn view(&self, _input: &Self::Input, context: &mut ViewContext<Self>) -> View {
        let s = self.lang.strings();
        let bounds = win::popup_bounds(420.0, 236.0, 12.0);
        context.window_title(format!("PausaPatch — {}", s.notice_change_title));
        context.window_visuals(
            WindowVisuals::new()
                .client_size(420.0, 236.0)
                .theme(WindowTheme::Dark)
                .initial_placement(WindowPlacement {
                    x: bounds.x,
                    y: bounds.y,
                    width: bounds.width,
                    height: bounds.height,
                    maximized: false,
                }),
        );

        let title = match self.notice.kind {
            NoticeKind::Reactivated => s.notice_reactivated_title,
            NoticeKind::Paused => s.notice_paused_title,
            NoticeKind::Mixed => s.notice_mixed_title,
        };
        let body = match self.notice.kind {
            NoticeKind::Reactivated => s.notice_reactivated_body,
            NoticeKind::Paused => s.notice_paused_body,
            NoticeKind::Mixed => s.notice_mixed_body,
        };
        let detail = {
            let transition = self.notice.detail_line(self.lang);
            let date = self.lang.format_date(self.notice.at);
            if transition.is_empty() {
                date
            } else {
                format!("{transition} · {date}")
            }
        };

        let header = Border::new()
            .height(36.0)
            .padding(Thickness::new(12.0, 0.0, 8.0, 0.0))
            .background(solid((0x0f, 0x6c, 0xbd)))
            .content(
                Grid::new()
                    .columns([GridLength::STAR, GridLength::Auto])
                    .children((
                        TextBlock::new()
                            .text("PausaPatch")
                            .font_size(12.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .foreground(solid((0xff, 0xff, 0xff)))
                            .grid_column(0)
                            .vertical_alignment(VerticalAlignment::Center),
                        Button::new()
                            .style(ButtonStyle::Subtle)
                            .content(
                                TextBlock::new()
                                    .text("✕")
                                    .font_size(13.0)
                                    .foreground(solid((0xff, 0xff, 0xff))),
                            )
                            .width(34.0)
                            .height(30.0)
                            .grid_column(1)
                            .on_click(context.message(NoticeMessage::Dismiss)),
                    )),
            );

        let icon = Border::new()
            .width(42.0)
            .height(42.0)
            .corner_radius(21.0)
            .background(alpha(34, AMBER))
            .content(
                TextBlock::new()
                    .text("!")
                    .font_size(20.0)
                    .font_weight(FontWeight::BOLD)
                    .foreground(solid(AMBER))
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .vertical_alignment(VerticalAlignment::Center),
            );

        let text_stack = StackPanel::new().spacing(2.0).children((
            TextBlock::new()
                .text(title)
                .font_size(14.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(solid((0xff, 0xff, 0xff))),
            TextBlock::new()
                .text(body)
                .font_size(11.5)
                .foreground(solid((0xb8, 0xb8, 0xb8)))
                .text_wrapping(TextWrapping::WrapWholeWords),
            TextBlock::new()
                .text(detail)
                .font_size(10.5)
                .font_family("Cascadia Mono, Consolas, monospace")
                .foreground(solid(MUTED))
                .text_trimming(TextTrimming::CharacterEllipsis)
                .margin(Thickness::new(0.0, 4.0, 0.0, 0.0)),
        ));

        let body_grid = Border::new()
            .padding(Thickness::new(16.0, 12.0, 16.0, 8.0))
            .content(
                Grid::new()
                    .columns([GridLength::Auto, GridLength::STAR])
                    .column_spacing(12.0)
                    .children((
                        icon.grid_column(0)
                            .vertical_alignment(VerticalAlignment::Top),
                        text_stack.grid_column(1),
                    )),
            );

        let mut buttons: Vec<View> = Vec::new();
        if matches!(
            self.notice.kind,
            NoticeKind::Reactivated | NoticeKind::Mixed
        ) {
            buttons.push(
                Button::new()
                    .style(ButtonStyle::Subtle)
                    .content(s.dismiss)
                    .on_click(context.message(NoticeMessage::Dismiss))
                    .into(),
            );
            buttons.push(
                Button::new()
                    .style(ButtonStyle::Accent)
                    .content(s.pause_again)
                    .on_click(context.message(NoticeMessage::PauseAgain))
                    .into(),
            );
        } else {
            buttons.push(
                Button::new()
                    .style(ButtonStyle::Accent)
                    .content(s.close)
                    .on_click(context.message(NoticeMessage::Dismiss))
                    .into(),
            );
        }

        let footer = Border::new()
            .padding(Thickness::new(16.0, 0.0, 16.0, 12.0))
            .content(
                Grid::new()
                    .columns([GridLength::STAR, GridLength::Auto])
                    .children((
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(6.0)
                            .grid_column(0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((
                                Ellipse::new().width(4.0).height(4.0).fill(solid(MUTED)),
                                TextBlock::new()
                                    .text(s.notice_stays)
                                    .font_size(9.5)
                                    .foreground(solid((0x7f, 0x7f, 0x7f)))
                                    .text_wrapping(TextWrapping::WrapWholeWords),
                            )),
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(8.0)
                            .grid_column(1)
                            .children(buttons),
                    )),
            );

        Border::new()
            .background(solid((0x2b, 0x2b, 0x2b)))
            .content(
                Grid::new()
                    .rows([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                    .children((
                        header.grid_row(0),
                        body_grid.grid_row(1),
                        footer.grid_row(2),
                    )),
            )
            .into()
    }
}

impl Drop for NoticeWindow {
    fn drop(&mut self) {
        self.notify(NoticeAction::Dismiss);
    }
}

impl Drop for LogWindow {
    fn drop(&mut self) {
        win::release_modal("PausaPatch");
    }
}

#[derive(Clone, PartialEq)]
struct LogInput {
    lang: Lang,
    lines: Vec<String>,
}

#[derive(Clone, Copy)]
enum LogMessage {
    Setup,
    CopyAll,
    OpenFolder,
    AskClear,
    ClearConfirmed(ContentDialogResult),
    Noop,
}

struct LogWindow {
    lines: Vec<String>,
    setup_timer: Option<ComponentTimer>,
    confirm_clear: bool,
}

impl Component for LogWindow {
    type Input = LogInput;
    type Message = LogMessage;

    fn create(input: &Self::Input, context: &ComponentContext<Self>) -> Self {
        Self {
            lines: input.lines.clone(),
            // La ventana aún no existe en `create`: se configura en el primer tick.
            setup_timer: Some(
                context.set_local_timeout(Duration::from_millis(80), || LogMessage::Setup),
            ),
            confirm_clear: false,
        }
    }

    fn input_changed(&mut self, input: &Self::Input, _context: &ComponentContext<Self>) {
        self.lines = input.lines.clone();
    }

    fn update(&mut self, message: LogMessage, context: &ComponentContext<Self>) {
        match message {
            LogMessage::Setup => {
                self.setup_timer = None;
                let _ = context.run_window(|handle| {
                    let hwnd = handle.as_raw() as _;
                    win::hide_on_close(hwnd);
                    win::make_modal(hwnd, "PausaPatch");
                    LogMessage::Noop
                });
            }
            LogMessage::Noop => {}
            LogMessage::CopyAll => {
                if let Err(error) = win::copy_text(&self.lines.join("\r\n")) {
                    log::log(format!("Could not copy the log: {error}"));
                }
            }
            LogMessage::OpenFolder => {
                win::reveal_in_explorer(&paths::app_log_path());
            }
            LogMessage::AskClear => self.confirm_clear = true,
            LogMessage::ClearConfirmed(result) => {
                self.confirm_clear = false;
                if matches!(result, ContentDialogResult::Primary) {
                    match std::fs::File::create(paths::app_log_path()) {
                        Ok(_) => {
                            // Se anota después de vaciar y luego se recarga,
                            // para que el aviso aparezca en la ventana.
                            log::log("Log cleared by the user.");
                            self.lines = log::read(400);
                        }
                        Err(error) => {
                            log::error(format!("Could not clear the log: {error}"));
                        }
                    }
                }
            }
        }
    }

    fn view(&self, input: &Self::Input, context: &mut ViewContext<Self>) -> View {
        let s = input.lang.strings();
        let (x, y) = win::centered_position(760.0, 560.0);
        context.window_visuals(
            WindowVisuals::new()
                .client_size(760.0, 560.0)
                .theme(WindowTheme::Dark)
                .initial_position(ScreenPoint::new(x, y)),
        );

        // Un único bloque de texto: así se puede seleccionar y copiar de una
        // vez, en lugar de línea a línea.
        let body = if self.lines.is_empty() {
            s.log_empty.to_string()
        } else {
            self.lines.join("\n")
        };
        let content = TextBlock::new()
            .text(body)
            .font_size(11.0)
            .font_family("Cascadia Mono, Consolas, monospace")
            .foreground(solid((0xc8, 0xc8, 0xc8)))
            .text_wrapping(TextWrapping::Wrap)
            .is_text_selection_enabled(true);

        let toolbar = Grid::new()
            .columns([
                GridLength::STAR,
                GridLength::Auto,
                GridLength::Auto,
                GridLength::Auto,
            ])
            .column_spacing(8.0)
            .children((
                TextBlock::new()
                    .text(input.lang.log_lines(self.lines.len()))
                    .font_size(11.0)
                    .foreground(solid(MUTED))
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center),
                Button::new()
                    .style(ButtonStyle::Subtle)
                    .content(s.log_open_folder)
                    .grid_column(1)
                    .on_click(context.message(LogMessage::OpenFolder)),
                Button::new()
                    .style(ButtonStyle::Subtle)
                    .content(s.log_clear)
                    .is_enabled(!self.lines.is_empty())
                    .grid_column(2)
                    .on_click(context.message(LogMessage::AskClear)),
                Button::new()
                    .content(s.copy_all)
                    .is_enabled(!self.lines.is_empty())
                    .grid_column(3)
                    .on_click(context.message(LogMessage::CopyAll)),
            ));

        let confirmation = ContentDialog::new()
            .title(s.log_clear)
            .content(
                TextBlock::new()
                    .text(s.log_clear_confirm)
                    .text_wrapping(TextWrapping::WrapWholeWords),
            )
            .primary_button_text(s.log_clear)
            .close_button_text(s.cancel)
            .default_button(ContentDialogButton::Close)
            .is_open(self.confirm_clear)
            .on_closed(context.callback(LogMessage::ClearConfirmed));

        context.window_frame(
            format!("PausaPatch — {}", s.log_title),
            Border::new()
                .padding(Thickness::new(20.0, 12.0, 20.0, 12.0))
                .background(solid(WINDOW_BG))
                .content(
                    Grid::new()
                        .rows([GridLength::Auto, GridLength::STAR])
                        .row_spacing(10.0)
                        .children((
                            toolbar.grid_row(0),
                            ScrollView::new()
                                .content(content)
                                .grid_row(1)
                                .horizontal_alignment(HorizontalAlignment::Stretch),
                        )),
                )
                .content_dialog(confirmation),
        )
    }
}

fn main() {
    // La tarea SYSTEM nos relanza con `--apply <acción> --result <ruta>`.
    if let Some((action, result)) = headless_request() {
        std::process::exit(apply::run(action, result));
    }
    if !single_instance() {
        return;
    }
    let _ = App::run_component::<PausaPatch>(());
}

fn headless_request() -> Option<(apply::Action, std::path::PathBuf)> {
    let mut arguments = std::env::args().skip(1);
    let mut action = None;
    let mut result = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            apply::APPLY_ARGUMENT => {
                action = arguments.next().and_then(|value| apply::Action::parse(&value));
            }
            apply::RESULT_ARGUMENT => {
                result = arguments.next().map(std::path::PathBuf::from);
            }
            _ => {}
        }
    }
    Some((action?, result?))
}

/// Evita dos copias simultáneas (dos iconos de bandeja y dos descargas).
/// Si ya hay una, intenta traer su ventana al frente.
fn single_instance() -> bool {
    use windows_sys::Win32::System::Threading::CreateMutexW;
    const ERROR_ALREADY_EXISTS: u32 = 183;

    let name: Vec<u16> = "Local\\PausaPatch.SingleInstance"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 1, name.as_ptr()) };
    if handle.is_null() {
        return true;
    }
    if unsafe { windows_sys::Win32::Foundation::GetLastError() } == ERROR_ALREADY_EXISTS {
        focus_existing_window();
        return false;
    }
    // El handle se deja abierto: vive hasta que termine el proceso.
    let _ = handle;
    true
}

fn focus_existing_window() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SW_RESTORE, ShowWindow};
    for title in ["PausaPatch", "PausaPatch — Configuración inicial"] {
        let title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
        if !hwnd.is_null() {
            unsafe { ShowWindow(hwnd, SW_RESTORE) };
            win::restore_window(hwnd);
            return;
        }
    }
}

fn overall_code(overall: Overall) -> &'static str {
    match overall {
        Overall::Paused => "paused",
        Overall::Active => "active",
        Overall::Mixed => "mixed",
        Overall::Unknown => "unknown",
    }
}

fn overall_from_code(code: &str) -> Option<Overall> {
    match code {
        "paused" => Some(Overall::Paused),
        "active" => Some(Overall::Active),
        "mixed" => Some(Overall::Mixed),
        "unknown" => Some(Overall::Unknown),
        _ => None,
    }
}

fn notice_kind(overall: Overall) -> Option<NoticeKind> {
    match overall {
        Overall::Paused => Some(NoticeKind::Paused),
        Overall::Active => Some(NoticeKind::Reactivated),
        Overall::Mixed => Some(NoticeKind::Mixed),
        Overall::Unknown => None,
    }
}

use gpui::{
    App, AppContext as _, Bounds, Context, Div, Entity, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render, Styled as _,
    TitlebarOptions, Window, WindowBounds, WindowOptions, actions, div, px, size,
};
use gpui_base::Placement;
use ui::{
    Button, ColorPicker, Divider, Dropdown, Form, Icon, IconAssets, Kbd, PRIMARY, Root, SHIFT,
    ScrollArea, Segment, SegmentedControl, Slider, Switch, Theme, ThemeMode, ToolTooltip,
    ToolTooltipExt, notice, theme,
};

actions!(ui_demo, [Quit]);

struct Demo {
    focus: FocusHandle,
    slider: Entity<Slider>,
    segment: usize,
    tooltip_segment: usize,
    button_toggled: bool,
    checked: bool,
    color: Option<u32>,
    auto_color: Option<u32>,
}

impl Demo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let slider = cx.new(|cx| Slider::new("Value", 0.0..=100.0, 50.0, 1.0, cx));
        Self {
            focus,
            slider,
            segment: 0,
            tooltip_segment: 0,
            button_toggled: false,
            checked: false,
            color: Some(theme::PALETTE[0].0),
            auto_color: None,
        }
    }

    fn switch(&self, id: &'static str, inverted: bool, cx: &mut Context<Self>) -> Switch {
        let listener = cx.listener(move |this, checked: &bool, _, cx| {
            this.checked = *checked ^ inverted;
            cx.notify();
        });
        Switch::new(id)
            .checked(self.checked ^ inverted)
            .accessibility_label(id)
            .on_change(move |checked, window, cx| listener(&checked, window, cx))
    }

    fn components(&self, cx: &mut Context<Self>) -> Div {
        column()
            .gap_6()
            .child(section(
                "Button",
                row().children([
                    Button::new("primary").primary().label("Primary"),
                    Button::new("secondary").label("Secondary"),
                    Button::new("ghost").ghost().label("Ghost"),
                    Button::icon("icon-button", Icon::Copy).accessibility_label("Copy"),
                    Button::icon("toggle-button", Icon::Square)
                        .accessibility_label("Rectangle")
                        .toggled(self.button_toggled)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.button_toggled = !this.button_toggled;
                            cx.notify();
                        })),
                    Button::icon("disabled-button", Icon::ArrowBackUp)
                        .accessibility_label("Undo")
                        .disabled(true),
                ]),
                cx,
            ))
            .child(section(
                "Dropdown",
                row().child(
                    Dropdown::new(
                        "save-menu",
                        Button::icon("save", Icon::ChevronDown)
                            .accessibility_label("Save options"),
                    )
                    .menu_label("Save options")
                    .items([
                        Button::new("save-png").ghost().label("Save as PNG"),
                        Button::new("save-jpg").ghost().label("Save as JPG"),
                    ]),
                ),
                cx,
            ))
            .child(section(
                "SegmentedControl",
                row().child(
                    SegmentedControl::new("fill-style", self.segment)
                        .accessibility_label("Fill style")
                        .items(
                            ["Outline", "Fill", "Both"]
                                .into_iter()
                                .enumerate()
                                .map(|(index, label)| Segment::new(index, label)),
                        )
                        .on_change(cx.listener(|this, index: &usize, _, cx| {
                            this.segment = *index;
                            cx.notify();
                        })),
                ),
                cx,
            ))
            .child(section(
                "Switch",
                row().gap_6().children(
                    [
                        ("switch", false, false),
                        ("switch-inverted", true, false),
                        ("disabled-switch", false, true),
                        ("disabled-switch-inverted", true, true),
                    ]
                    .into_iter()
                    .map(|(id, inverted, disabled)| {
                        self.switch(id, inverted, cx).disabled(disabled)
                    }),
                ),
                cx,
            ))
            .child(section("Slider", self.slider.clone(), cx))
            .child(section(
                "ColorPicker",
                row()
                    .gap_6()
                    .child(
                        ColorPicker::new("colors")
                            .selected(self.color)
                            .on_change(cx.listener(|this, color: &Option<u32>, _, cx| {
                                this.color = *color;
                                cx.notify();
                            })),
                    )
                    .child(
                        ColorPicker::new("auto-colors")
                            .allow_auto(true)
                            .selected(self.auto_color)
                            .on_change(cx.listener(|this, color: &Option<u32>, _, cx| {
                                this.auto_color = *color;
                                cx.notify();
                            })),
                    ),
                cx,
            ))
            .child(section(
                "Kbd",
                row()
                    .child(Kbd::new([PRIMARY, "C"]))
                    .child(Kbd::new(["Enter", "or", "Esc"])),
                cx,
            ))
            .child(section(
                "Tooltip",
                row()
                    .child(
                        Button::new("tooltip-copy").label("Copy").tool_tooltip(
                            ToolTooltip::new("Copy")
                                .description("Copy image")
                                .shortcut([PRIMARY, "C"]),
                        ),
                    )
                    .child(
                        SegmentedControl::new("tooltip-fill-style", self.tooltip_segment)
                            .accessibility_label("Fill style")
                            .items(
                                [
                                    (
                                        "Outline",
                                        "Outline only",
                                        "Draw a border around an area while keeping the screenshot visible inside. Useful for calling out a button or a line of text.",
                                        "1",
                                    ),
                                    (
                                        "Fill",
                                        "Fill only",
                                        "Draw a solid shape in the selected color without a border. Useful for covering a distracting area or adding a color block.",
                                        "2",
                                    ),
                                    (
                                        "Both",
                                        "Outline and fill",
                                        "Draw a filled shape with a border. Useful for creating a label background or making an annotation stand out.",
                                        "3",
                                    ),
                                ]
                                .into_iter()
                                .enumerate()
                                .map(|(index, (label, title, description, shortcut))| {
                                    Segment::new(index, label).tooltip(
                                        ToolTooltip::new(title)
                                            .description(description)
                                            .shortcut([PRIMARY, shortcut])
                                            .hint("Constrain proportions", [SHIFT])
                                            .hint("Undo last annotation", [PRIMARY, "Z"])
                                            .placement(Placement::Top),
                                    )
                                }),
                            )
                            .on_change(cx.listener(|this, index: &usize, _, cx| {
                                this.tooltip_segment = *index;
                                cx.notify();
                            })),
                    ),
                cx,
            ))
            .child(section(
                "Notice",
                column()
                    .child(notice("Notice", false, cx))
                    .child(notice("Error", true, cx)),
                cx,
            ))
            .child(section(
                "Form",
                Form::new()
                    .title("Settings")
                    .field("Setting", "Description", self.switch("form-switch", false, cx))
                    .field_footer(notice("Changes take effect immediately.", false, cx))
                    .field(
                        "Take Screenshot",
                        "Global shortcut",
                        Kbd::new([PRIMARY, SHIFT, "2"]),
                    )
                    .field_footer(notice(
                        "Screen recording permission is required to use this shortcut.",
                        true,
                        cx,
                    )),
                cx,
            ))
            .child(section(
                "Divider",
                column().child(Divider::horizontal()).child(
                    row()
                        .h_7()
                        .child("Left")
                        .child(Divider::vertical())
                        .child("Right"),
                ),
                cx,
            ))
            .child(section(
                "Icon",
                row().children(Icon::ALL.iter().copied()),
                cx,
            ))
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let controls = row().child(
            SegmentedControl::new("theme", theme::mode(cx))
                .accessibility_label("Theme")
                .items([
                    Segment::new(ThemeMode::System, "System"),
                    Segment::new(ThemeMode::Light, "Light"),
                    Segment::new(ThemeMode::Dark, "Dark"),
                ])
                .on_change(|mode, window, cx| theme::set_mode(*mode, Some(window), cx)),
        );
        column()
            .gap_0()
            .track_focus(&self.focus)
            .size_full()
            .on_action(|_: &Quit, _, cx| cx.quit())
            .child(
                row()
                    .justify_between()
                    .flex_shrink_0()
                    .p_4()
                    .border_b_1()
                    .border_color(Theme::global(cx).tokens.colors.border)
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("UI Components"),
                    )
                    .child(controls),
            )
            .child(div().flex_1().min_h_0().child(ScrollArea::new(
                "components",
                div().p_6().child(self.components(cx)),
            )))
    }
}

fn column() -> Div {
    div().flex().flex_col().gap_3()
}

fn row() -> Div {
    div().flex().flex_wrap().items_center().gap_3()
}

fn section(title: &'static str, content: impl IntoElement, cx: &App) -> Div {
    column()
        .gap_2()
        .flex_shrink_0()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .p_4()
                .border_1()
                .border_color(Theme::global(cx).tokens.colors.border)
                .rounded_lg()
                .child(content),
        )
}

fn main() {
    gpui_platform::application()
        .with_assets(IconAssets)
        .run(|cx: &mut App| {
            ui::init(cx);
            cx.bind_keys([
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "cmd-q"
                    } else {
                        "ctrl-q"
                    },
                    Quit,
                    None,
                ),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "cmd-w"
                    } else {
                        "ctrl-w"
                    },
                    Quit,
                    None,
                ),
            ]);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let bounds = Bounds::centered(None, size(px(1000.), px(800.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(640.), px(480.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("UI Components".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    theme::observe_window(window, cx);
                    let demo = cx.new(|cx| Demo::new(window, cx));
                    cx.new(|cx| Root::new(demo, window, cx))
                },
            )
            .expect("failed to open the UI demo window");
            cx.activate(true);
        });
}

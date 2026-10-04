mod adjustments;
mod commands;
mod crop_ui;
mod effects;
mod export;
mod import;
mod interaction;
mod measurement_loupe;
mod options;
mod options_panel;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod pinned;
mod preferences;
mod state;
mod text_edit;
mod tools;
mod ui;
mod view;
mod workspace;

use self::export::Destination;
use self::options_panel::OptionsPanel;
use self::preferences::ToolPreferences;
use self::state::SavedEdits;
pub(crate) use self::state::ScreenshotState;
use self::ui::show_error;
use crate::canvas::ImageViewport;
use crate::color_format::ColorFormat;
use crate::crop::CropSelection;
use crate::dimensions::{DimensionDisplay, DimensionMode};
use crate::editor::{
    Editor, EffectStrength, LineStyle, Point as ImagePoint, RedactionStyle, ResizeHandle, Stroke,
    TailDirection, TextAnnotation, Tool,
};
use crate::erase::BackgroundCache;
use crate::export::ExportCache;
use crate::image_processing::SourceImage;
use crate::magnifier::{Magnifier, PreviewCache};
use crate::measurement::{MeasurementAxis, MeasurementCache, MeasurementDrag, MeasurementMode};
use crate::mosaic::Mosaic;
use crate::theme::PALETTE as COLORS;
use crate::{desktop, theme};
use ::ui::{Slider, SliderEvent};
pub(crate) use commands::{SaveImage, install};
use gpui::{prelude::*, *};
use gpui_base::input::TextareaState;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

const DEFAULT_NUMBER_SIZE: f32 = 24.0;
const DEFAULT_TEXT_SIZE: f32 = 16.0;
const MIN_STROKE_WIDTH: f32 = 2.0;
const MAX_STROKE_WIDTH: f32 = 24.0;
const STROKE_WIDTH_STEP: f32 = 2.0;
const DEFAULT_STROKE_WIDTH: f32 = 4.0;

gpui::actions!(screenshot, [CopyPixelColor]);

struct TextEdit {
    input: Entity<TextareaState>,
    position: ImagePoint,
    text: TextAnnotation,
    color: u32,
    existing: bool,
    preview: Option<Stroke>,
    input_size: ImagePoint,
    _subscriptions: Vec<Subscription>,
}

pub(crate) struct ScreenshotEditor {
    screenshot: Rc<RefCell<ScreenshotState>>,
    editor: Editor,
    crop: Option<CropSelection>,
    source: SourceImage,
    dimension_mode: DimensionMode,
    blurred_images: HashMap<EffectStrength, Arc<RenderImage>>,
    preparing_blur: Option<EffectStrength>,
    preparing_mosaic: bool,
    mosaic_ready: bool,
    image_alignment: Point<f32>,
    mosaic: Arc<Mosaic>,
    overlay_images: HashMap<u64, Arc<RenderImage>>,
    next_image_id: u64,
    image_history_revision: u64,
    importing_image: bool,
    exporting_image: bool,
    export_cache: Option<ExportCache>,
    tool: Option<Tool>,
    preferences: Rc<ToolPreferences>,
    toolbar_drag_pending: bool,
    options_panel: Rc<RefCell<OptionsPanel>>,
    hovered_handle: Option<ResizeHandle>,
    hover_position: Option<ImagePoint>,
    sampled_color: u32,
    color_copy_feedback: Option<Task<()>>,
    sampling_position: Option<Point<Pixels>>,
    preview_position: Option<ImagePoint>,
    measurement_cache: RefCell<MeasurementCache>,
    measurement_drag: Option<MeasurementDrag>,
    magnifier_previews: Rc<RefCell<PreviewCache>>,
    adjustment_scroll: Option<(usize, f32, std::time::Instant)>,
    fill_previews: Rc<RefCell<BackgroundCache>>,
    adjustment_slider: Entity<Slider>,
    text_edit: Option<TextEdit>,
    flipped_images: HashMap<(u64, bool, bool), Arc<RenderImage>>,
    preparing_flips: Option<(u64, bool, bool)>,
    viewport: Rc<Cell<Option<ImageViewport>>>,
    focus: FocusHandle,
    escape_pressed: bool,
    pressed_nudge_keys: HashSet<String>,
    _subscriptions: Vec<Subscription>,
}

impl ScreenshotEditor {
    fn dimension_display(&self) -> DimensionDisplay {
        self.source.pixel_scale.display(self.dimension_mode)
    }

    pub(crate) fn new(
        screenshot: Rc<RefCell<ScreenshotState>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        #[cfg(target_os = "macos")]
        if let Err(error) = desktop::center_window_controls(window, theme::TOOLBAR_HEIGHT) {
            eprintln!("Unable to position window controls: {error}");
        }
        let preferences = ToolPreferences::shared(cx);
        let source = screenshot.borrow().source.clone();
        let saved = screenshot.borrow_mut().edits.take();
        let dimensions = source.pixels.dimensions();
        cx.on_release(|this, _| {
            this.screenshot.borrow_mut().save_edits(
                &this.editor,
                &this.overlay_images,
                this.next_image_id,
                this.image_alignment,
            );
        })
        .detach();
        let editor = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            let _ = editor.update(cx, |this, cx| this.prepare_close(window, cx));
            true
        });
        window
            .observe_release(&cx.entity(), cx, |this, window, cx| {
                let mut screenshot = this.screenshot.borrow_mut();
                if screenshot
                    .window
                    .is_some_and(|(handle, _)| handle == window.window_handle())
                {
                    screenshot.window = None;
                }
                drop(screenshot);
                for image in std::iter::once(&this.source.rendered)
                    .chain(this.blurred_images.values())
                    .chain(this.overlay_images.values())
                    .chain(this.flipped_images.values())
                {
                    cx.drop_image(image.clone(), Some(window));
                }
                this.magnifier_previews.borrow_mut().clear(window, cx);
            })
            .detach();
        let dimension_subscription = cx.observe_global::<DimensionMode>(|this, cx| {
            this.dimension_mode = DimensionMode::current(cx);
            let grid = this.dimension_display().grid();
            this.editor.set_grid(grid);
            if let Some(crop) = this.crop.as_mut() {
                crop.set_grid(grid);
            }
            cx.notify();
        });
        let color_subscription = cx.observe_global::<ColorFormat>(|_, cx| cx.notify());
        let focus = cx.focus_handle();
        let blur_subscription = cx.on_blur(&focus, window, |this, _, _| {
            this.pressed_nudge_keys.clear();
        });
        let adjustment_slider = cx.new(|cx| {
            Slider::new(
                "Stroke width (logical pixels)",
                MIN_STROKE_WIDTH..=MAX_STROKE_WIDTH,
                DEFAULT_STROKE_WIDTH,
                STROKE_WIDTH_STEP,
                cx,
            )
        });
        let adjustment_subscription =
            cx.subscribe_in(&adjustment_slider, window, |this, _, event, window, cx| {
                match event {
                    SliderEvent::Change(value) => {
                        if this.active_tool() == Some(Tool::Magnifier) {
                            this.preferences.magnifier_zoom.set(*value);
                            this.editor.preview_magnifier_zoom(*value);
                        } else if let Some(tool) =
                            this.active_tool().filter(|tool| tool.is_filled_shape())
                        {
                            let opacity = (*value / 10.0).round().clamp(0.0, 10.0) / 10.0;
                            this.preferences.set_fill_opacity(tool, opacity);
                            this.editor.preview_fill_opacity(opacity);
                        } else if this.active_tool().is_some_and(Tool::has_strength) {
                            let index = (*value as usize).min(EffectStrength::ALL.len() - 1);
                            this.preferences
                                .effect_strength
                                .set(EffectStrength::ALL[index]);
                            this.editor
                                .preview_effect_strength(this.preferences.effect_strength.get());
                        } else if this.active_tool().is_some_and(Tool::has_width) {
                            this.preferences.stroke_width.set(*value);
                            let width = this.image_stroke_width();
                            this.editor.preview_selected_width(width);
                        }
                    }
                    SliderEvent::Release => {
                        this.editor.finish_style_change();
                    }
                    SliderEvent::Cancel => {
                        this.editor.cancel();
                        this.focus.focus(window, cx);
                    }
                }
                cx.notify();
            });
        focus.focus(window, cx);
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                desktop::window_activated(window.window_handle(), cx);
            }
            if !window.is_window_active() {
                this.escape_pressed = false;
                this.pressed_nudge_keys.clear();
                this.sampling_position = None;
                this.clear_hover(cx);
                let measurement_canceled = this.measurement_drag.take().is_some();
                this.toolbar_drag_pending = false;
                let panel_canceled = this.options_panel.borrow_mut().end_drag();
                let crop_canceled = this.crop.as_mut().is_some_and(CropSelection::cancel_drag);
                let canceled = this.editor.cancel() || crop_canceled;
                if canceled || panel_canceled || measurement_canceled {
                    cx.notify();
                }
            }
        });
        let SavedEdits {
            mut editor,
            overlay_images,
            next_image_id,
            image_alignment,
        } = saved.unwrap_or_else(|| SavedEdits {
            editor: Editor::new(dimensions.0 as f32, dimensions.1 as f32),
            overlay_images: HashMap::new(),
            next_image_id: 0,
            image_alignment: point(0.5, 0.5),
        });
        editor.set_grid(
            source
                .pixel_scale
                .display(DimensionMode::current(cx))
                .grid(),
        );
        let fill_previews = Rc::new(RefCell::new(BackgroundCache::new(source.pixels.clone())));
        Self {
            screenshot,
            editor,
            crop: None,
            source,
            dimension_mode: DimensionMode::current(cx),
            blurred_images: HashMap::new(),
            preparing_blur: None,
            preparing_mosaic: false,
            mosaic_ready: false,
            image_alignment,
            mosaic: Arc::new(Mosaic::new(&image::RgbaImage::new(0, 0))),
            overlay_images,
            next_image_id,
            image_history_revision: 0,
            importing_image: false,
            exporting_image: false,
            export_cache: None,
            tool: None,
            preferences,
            toolbar_drag_pending: false,
            options_panel: Rc::default(),
            hovered_handle: None,
            hover_position: None,
            sampled_color: theme::SURFACE,
            color_copy_feedback: None,
            sampling_position: None,
            preview_position: None,
            measurement_cache: RefCell::default(),
            measurement_drag: None,
            magnifier_previews: Rc::default(),
            adjustment_scroll: None,
            fill_previews,
            adjustment_slider,
            text_edit: None,
            flipped_images: HashMap::new(),
            preparing_flips: None,
            viewport: Rc::new(Cell::new(None)),
            focus,
            escape_pressed: false,
            pressed_nudge_keys: HashSet::new(),
            _subscriptions: vec![
                activation,
                adjustment_subscription,
                dimension_subscription,
                color_subscription,
                blur_subscription,
            ],
        }
    }

    fn prepare_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_text(true, window, cx);
        self.editor.cancel();
        crate::capture::remember_editor(self.screenshot.clone(), cx);
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prepare_close(window, cx);
        window.remove_window();
    }

    pub(crate) fn set_image_alignment(&mut self, alignment: Point<f32>, cx: &mut Context<Self>) {
        self.image_alignment = alignment;
        cx.notify();
    }
}

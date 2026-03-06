mod correction;
mod error;
mod pipeline;
mod ui;
mod upload;
mod utils;

use crate::{
    error::Error,
    pipeline::{EncodedImage, Intermediate, Step},
    ui::{preview::Preview, viewer::Viewer},
    upload::{State, Update, Upload},
    utils::dynamic_image_to_handle,
};

use iced::{
    Element, Function, Length, Subscription, Task,
    time::Instant,
    widget::{button, column, container, grid, row, scrollable, stack, text},
    window,
};

#[non_exhaustive]
#[derive(Debug, Clone)]
enum Message {
    Upload,
    UploadUpdated(Update),
    Process(Result<Intermediate, Error>),
    BlurhashDecoded(Intermediate, EncodedImage),
    ThumbnailHovered(Step, bool),
    Open(Step),
    Close,
    Animate,
}
struct Img {
    upload: Upload,
    now: Instant,
    viewer: Viewer,
    intermediates: Vec<Intermediate>,
}

impl Img {
    fn new() -> Self {
        Self {
            upload: Upload::new(),
            now: Instant::now(),
            viewer: Viewer::new(),
            intermediates: Vec::new(),
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let is_animating = self
            .intermediates
            .iter()
            .any(|i| i.preview.is_animating(self.now))
            || self.viewer.is_animating(self.now)
            || self.upload.is_animating(self.now);

        if is_animating {
            window::frames().map(|_| Message::Animate)
        } else {
            Subscription::none()
        }
    }

    fn update(&mut self, message: Message, now: Instant) -> Task<Message> {
        self.now = now;

        match message {
            Message::Upload => {
                let task = self.upload.start();

                task.map(Message::UploadUpdated)
            }
            Message::UploadUpdated(update) => {
                self.upload.update(update);

                Task::none()
            }
            Message::Process(Ok(inter)) => {
                if let Some(last) = self.intermediates.last_mut() {
                    *last = inter.clone();
                }
                match inter.current_step {
                    Step::FruitletCounting => Task::none(),
                    _ => Task::sip(
                        inter.clone().process(),
                        Message::BlurhashDecoded.with(inter),
                        Message::Process,
                    ),
                }
            }
            Message::BlurhashDecoded(mut inter, blurhash) => {
                inter.preview = Preview::loading(blurhash, self.now);
                self.intermediates.push(inter);

                Task::none()
            }
            Message::ThumbnailHovered(step, is_hovered) => {
                if let Some(i) = self
                    .intermediates
                    .iter_mut()
                    .find(|i| i.current_step == step)
                {
                    i.preview.toggle_zoom(is_hovered, self.now);
                } else if let State::Finished(i) = &self.upload.state {
                    i.preview.clone().toggle_zoom(is_hovered, self.now);
                }

                Task::none()
            }
            Message::Open(step) => {
                if let Some(intermediate) = self
                    .intermediates
                    .iter()
                    .find(|i| i.current_step == step)
                    .cloned()
                {
                    self.viewer.show(
                        dynamic_image_to_handle(&intermediate.preview.into()),
                        self.now,
                    );
                } else if let State::Finished(i) = &self.upload.state {
                    self.viewer
                        .show(dynamic_image_to_handle(&i.clone().preview.into()), self.now);
                }

                Task::none()
            }
            Message::Close => {
                self.viewer.close(self.now);

                Task::none()
            }
            Message::Animate => Task::none(),
            Message::Process(Err(e)) => {
                log::error!("Processing failed: {e:?}");
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let controls = column![
            button("Choose the image").on_press(Message::Upload),
            self.upload.view(),
            match &self.upload.state {
                State::Finished(i) => Some(i.card(self.now)),
                _ => None,
            },
            match &self.upload.state {
                State::Finished(i) => {
                    Some(button("Do it!").on_press(Message::Process(Ok(*i.clone()))))
                }
                _ => None,
            }
        ]
        .spacing(10)
        .width(Length::FillPortion(1));

        let metrics_view = if let Some(metrics) = self.intermediates.iter().find_map(|i| {
            if i.current_step == Step::FruitletCounting {
                i.metrics.as_ref()
            } else {
                None
            }
        }) {
            let a_eq_str = metrics
                .a_eq
                .map_or("-".to_string(), |v| format!("{v:.2} mm"));
            let b_eq_str = metrics
                .b_eq
                .map_or("-".to_string(), |v| format!("{v:.2} mm"));
            let alpha_str = metrics
                .alpha
                .map_or("-".to_string(), |v| format!("{v:.3} rad"));
            let surface_area_str = metrics
                .surface_area
                .map_or("-".to_string(), |v| format!("{v:.2} mm²"));
            let n_total_str = metrics.n_total.map_or("-".to_string(), |v| format!("{v}"));

            column![
                text("Fruitlet Metrics").size(24),
                row![
                    text("Major Length:"),
                    text(format!("{:.2} mm", metrics.major_length))
                ]
                .spacing(20),
                row![
                    text("Minor Length:"),
                    text(format!("{:.2} mm", metrics.minor_length))
                ]
                .spacing(20),
                row![
                    text("Volume:"),
                    text(format!("{:.2} mm³", metrics.volume))
                ]
                .spacing(20),
                row![text("a_eq:"), text(a_eq_str)].spacing(20),
                row![text("b_eq:"), text(b_eq_str)].spacing(20),
                row![text("α:"), text(alpha_str)].spacing(20),
                row![text("Surface Area:"), text(surface_area_str)].spacing(20),
                row![text("N_total:"), text(n_total_str)].spacing(20),
            ]
            .spacing(10)
        } else {
            column![
                text("Fruitlet Metrics").size(24),
                row![text("Major Length:"), text("-")].spacing(20),
                row![text("Minor Length:"), text("-")].spacing(20),
                row![text("Volume:"), text("-")].spacing(20),
                row![text("a_eq:"), text("-")].spacing(20),
                row![text("b_eq:"), text("-")].spacing(20),
                row![text("α:"), text("-")].spacing(20),
                row![text("Surface Area:"), text("-")].spacing(20),
                row![text("N_total:"), text("-")].spacing(20),
            ]
            .spacing(10)
        };

        let content = container(
            row![
                controls,
                scrollable(
                    grid(self.intermediates.iter().map(|i| i.card(self.now)))
                        .columns(1)
                        .spacing(5)
                )
                .width(Length::FillPortion(1)),
                container(metrics_view).width(Length::FillPortion(8))
            ]
            .spacing(10),
        );

        let viewer = self.viewer.view(self.now);

        stack![content, viewer].into()
    }
}

fn main() -> iced::Result {
    console_log::init().expect("Initialize logger");
    console_error_panic_hook::set_once();

    iced::application::timed(Img::new, Img::update, Img::subscription, Img::view)
        .centered()
        .run()
}

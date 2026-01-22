use ::image::{DynamicImage, EncodableLayout, GrayImage, Luma, Rgba};
use gloo_timers::future::TimeoutFuture;
use iced::{
    Color, ContentFit, Element, Fill, Length, Shadow,
    time::Instant,
    widget::{button, container, float, image, mouse_area, space, stack},
};
use image_debug_utils::{
    contours::{remove_hypotenuse_in_place, sort_by_perimeters_owned},
    rect::to_axis_aligned_bounding_box,
    region_labelling::draw_principal_connected_components,
};
use imageproc::{
    compose::crop_parallel,
    contours::{self, BorderType},
    contrast::adaptive_threshold,
    distance_transform::Norm,
    filter::{gaussian_blur_f32, median_filter},
    geometry::min_area_rect,
    morphology::{close, close_mut, open, open_mut},
    region_labelling::{Connectivity, connected_components},
};
use sipper::{Straw, sipper};

use crate::{Message, Preview, error::Error, utils::dynamic_image_to_handle};

pub(crate) type EncodedImage = Vec<u8>;

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    Original,
    Smoothing,
    Binary,
    FindContours,
    Connectivity,
    Final,
}

#[derive(Clone, Debug)]
pub(crate) struct Intermediate {
    pub(crate) current_step: Step,
    pub(crate) preview: Preview,
}

impl Intermediate {
    pub(crate) fn process(self) -> impl Straw<Self, EncodedImage, Error> {
        sipper(async move |mut sender| {
            let image: DynamicImage = self.preview.into();
            let blurhash = blurhash::encode(
                4,
                3,
                image.width(),
                image.height(),
                image.to_rgba8().as_bytes(),
            )
            .unwrap();

            TimeoutFuture::new(1000).await;
            let _ = sender
                .send(blurhash::decode(&blurhash, 20, 20, 1.0).unwrap())
                .await;
            TimeoutFuture::new(1000).await;

            match self.current_step {
                Step::Original => Ok(Intermediate {
                    current_step: Step::Smoothing,
                    preview: Preview::ready(
                        gaussian_blur_f32(&median_filter(&image.to_rgba8(), 1, 1), 1.0).into(),
                        Instant::now(),
                    ),
                }),
                Step::Smoothing => Ok(Intermediate {
                    current_step: Step::Binary,
                    preview: Preview::ready(
                        adaptive_threshold(&image.clone().to_luma8(), 10, 0).into(),
                        Instant::now(),
                    ),
                }),
                Step::Binary => {
                    let gray = image.to_luma8();
                    let opened = open(&gray, Norm::L2, 1);
                    let closed = close(&opened, Norm::L2, 1);

                    // let black_hat_res: Vec<u8> = closed
                    //     .as_raw()
                    //     .iter()
                    //     .zip(opened.as_raw())
                    //     .map(|(c, o)| c.saturating_sub(*o))
                    //     .collect();

                    Ok(Intermediate {
                        current_step: Step::FindContours,
                        preview: Preview::ready(
                            // GrayImage::from_raw(gray.width(), gray.height(), black_hat_res)
                            //     .unwrap()
                            //     .into(),
                            closed.into(),
                            Instant::now(),
                        ),
                    })
                }
                Step::FindContours => {
                    let gray = image.to_luma8();
                    let mut contours = contours::find_contours::<i32>(&gray);
                    remove_hypotenuse_in_place(&mut contours, 5.0, Some(BorderType::Hole));
                    let sorted = sort_by_perimeters_owned(contours);

                    Ok(Intermediate {
                        current_step: Step::Connectivity,
                        preview: Preview::ready(
                            crop_parallel(
                                &gray,
                                to_axis_aligned_bounding_box(&min_area_rect(
                                    &sorted.first().unwrap().0.points,
                                )),
                            )
                            .into(),
                            Instant::now(),
                        ),
                    })
                }
                Step::Connectivity => {
                    let components =
                        connected_components(&image.to_luma8(), Connectivity::Eight, Luma([0u8]));
                    Ok(Intermediate {
                        current_step: Step::Final,
                        preview: Preview::ready(
                            draw_principal_connected_components(
                                &components,
                                20,
                                Rgba([0, 0, 0, 255]),
                            )
                            .into(),
                            Instant::now(),
                        ),
                    })
                }
                Step::Final => unreachable!(),
            }
        })
    }
    pub(crate) fn card(&self, now: Instant) -> Element<'_, Message> {
        let image = {
            let thumbnail: Element<'_, _> = if let Preview::Ready { result_img, .. } = &self.preview
            {
                float(
                    image(dynamic_image_to_handle(&result_img.img))
                        .width(Fill)
                        .content_fit(ContentFit::Contain)
                        .opacity(result_img.fade_in.interpolate(0.0, 1.0, now)),
                )
                .scale(result_img.zoom.interpolate(1.0, 1.1, now))
                .translate(move |bounds, viewport| {
                    bounds.zoom(1.1).offset(&viewport.shrink(10))
                        * result_img.zoom.interpolate(0.0, 1.0, now)
                })
                .style(move |_theme| float::Style {
                    shadow: Shadow {
                        color: Color::BLACK.scale_alpha(result_img.zoom.interpolate(0.0, 1.0, now)),
                        blur_radius: result_img.zoom.interpolate(0.0, 20.0, now),
                        ..Shadow::default()
                    },
                    ..float::Style::default()
                })
                .into()
            } else {
                space::horizontal().into()
            };

            if let Some(blurhash) = self.preview.blurhash(now) {
                let blurhash = image(&blurhash.handle)
                    .width(Fill)
                    .height(Fill)
                    .content_fit(ContentFit::Fill)
                    .opacity(blurhash.fade_in.interpolate(0.0, 1.0, now));

                stack![blurhash, thumbnail].into()
            } else {
                thumbnail
            }
        };

        let card = mouse_area(container(image).style(container::dark))
            .on_enter(Message::ThumbnailHovered(self.current_step.clone(), true))
            .on_exit(Message::ThumbnailHovered(self.current_step.clone(), false));

        let is_result = matches!(self.preview, Preview::Ready { .. });

        button(card)
            .on_press_maybe(is_result.then_some(Message::Open(self.current_step.clone())))
            .padding(0)
            .style(button::text)
            .into()
    }
}

// impl Intermediate {
//     pub(crate) fn gray(&mut self) {
//         self.gray =
//             DynamicImage::ImageLuma8(image::load_from_memory(&self.original).unwrap().to_luma8())
//     }

//     fn blur(&mut self) {
//         // https://docs.rs/image/0.25.6/src/image/imageops/sample.rs.html#1004
//         self.blur = DynamicImage::ImageLuma8(blur(&self.gray.to_luma8(), 7.0));
//     }
// }
